use crate::{
    App,
    auth::{Principal, hash, principal_for_hash, same_origin},
    error::{ApiError, Result},
    store::Scope,
};
use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::Response,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub async fn upgrade(
    State(app): State<Arc<App>>,
    principal: Principal,
    Path(path): Path<HashMap<String, String>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    same_origin(&app, &headers)?;
    let scope = crate::routes::scope(&path)?;
    if !app.store.role(scope, principal.user).await?.execute() {
        return Err(ApiError::denied());
    }
    let expected_execution = path
        .get("execution_id")
        .map(|_| crate::routes::id(&path, "execution_id"))
        .transpose()?;
    if path.contains_key("execution_id") {
        let id = crate::routes::id(&path, "execution_id")?;
        let owned:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM executions WHERE organization_id=$1 AND workspace_id=$2 AND id=$3 AND user_id=$4 AND login_hash=$5)").bind(scope.organization).bind(scope.workspace).bind(id).bind(principal.user).bind(&principal.login).fetch_one(&app.store.pool).await?;
        if !owned {
            return Err(ApiError::missing());
        }
    }
    let permit = app
        .socket_capacity
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "STREAM_CAPACITY",
                "Stream capacity reached",
            )
        })?;
    Ok(upgrade
        .protocols(["studio.v1"])
        .max_message_size(8192)
        .max_frame_size(8192)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            let _ = serve(app, scope, principal, socket, expected_execution).await;
        }))
}
async fn send(socket: &mut WebSocket, value: Value) -> Result<()> {
    let text = serde_json::to_string(&value).map_err(|_| ApiError::unavailable())?;
    if text.len() > 2 * 1024 * 1024 {
        return Err(ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "PAGE_CAPACITY_EXCEEDED",
            "Serialized page exceeds 2 MiB",
        ));
    }
    tokio::time::timeout(
        Duration::from_secs(5),
        socket.send(Message::Text(text.into())),
    )
    .await
    .map_err(|_| ApiError::unavailable())?
    .map_err(|_| ApiError::unavailable())?;
    Ok(())
}
async fn authorized(app: &App, scope: Scope, owner: &Principal) -> Result<()> {
    principal_for_hash(app, owner.login.clone()).await?;
    if !app.store.role(scope, owner.user).await?.execute() {
        return Err(ApiError::denied());
    }
    Ok(())
}
fn uuid(value: &Value, key: &str) -> Result<Uuid> {
    value[key]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(ApiError::invalid)
}
async fn serve(
    app: Arc<App>,
    scope: Scope,
    mut owner: Principal,
    mut socket: WebSocket,
    expected_execution: Option<Uuid>,
) -> Result<()> {
    let hello = tokio::time::timeout(Duration::from_secs(5), socket.recv())
        .await
        .map_err(|_| ApiError::auth())?
        .ok_or_else(ApiError::auth)?
        .map_err(|_| ApiError::auth())?;
    let Message::Text(hello) = hello else {
        return Err(ApiError::auth());
    };
    let hello: Value = serde_json::from_str(&hello).map_err(|_| ApiError::auth())?;
    if hello["type"] != "hello"
        || hello["version"] != 1
        || !hello["csrf"]
            .as_str()
            .is_some_and(|csrf| bool::from(hash(csrf).as_slice().ct_eq(&owner.csrf_hash)))
    {
        return Err(ApiError::auth());
    }
    let tab = hello["tab_token"]
        .as_str()
        .filter(|token| token.len() == 43)
        .ok_or_else(ApiError::auth)?;
    owner.tab = Some(hash(tab));
    if let Some(id) = expected_execution {
        app.runtime.snapshot(scope, &owner, id)?;
    }
    authorized(&app, scope, &owner).await?;
    send(&mut socket, json!({"type":"ready","version":1})).await?;
    let mut watches: HashMap<Uuid, (db_types::QueryStatus, u64)> = HashMap::new();
    let mut sequence = 0u64;
    let mut outstanding: Option<(u64, Instant, Uuid)> = None;
    let mut pending_page: Option<Value> = None;
    let mut interval = tokio::time::interval(Duration::from_millis(250));
    loop {
        tokio::select! {
            _=app.shutdown.cancelled()=>break,
            message=socket.recv()=> {
                let Some(message)=message else{break;};
                let message=message.map_err(|_|ApiError::unavailable())?;
                if matches!(message,Message::Close(_)){break;}
                if let Message::Ping(value)=message {socket.send(Message::Pong(value)).await.map_err(|_|ApiError::unavailable())?;continue;}
                let Message::Text(message)=message else{return Err(ApiError::invalid());};
                let value:Value=serde_json::from_str(&message).map_err(|_|ApiError::invalid())?;
                authorized(&app,scope,&owner).await?;
                app.limiter.check(format!("ws:{}",owner.user),2400)?;
                match value["type"].as_str() {
                    Some("watch")=> {
                        let id=uuid(&value,"execution_id")?;app.runtime.snapshot(scope,&owner,id)?;
                        if watches.len()>=16 && !watches.contains_key(&id){return Err(ApiError::invalid());}
                        watches.insert(id,(db_types::QueryStatus::Created,u64::MAX));
                    }
                    Some("unwatch")=>{watches.remove(&uuid(&value,"execution_id")?);}
                    Some("ack")=> {
                        let ack=value["sequence"].as_u64().ok_or_else(ApiError::invalid)?;
                        if outstanding.is_some_and(|(expected,_,_)|expected==ack){outstanding=None;}
                        else{return Err(ApiError::new(axum::http::StatusCode::CONFLICT,"ACK_SEQUENCE_INVALID","ACK is stale or invalid"));}
                    }
                    Some("page")=> {
                        if pending_page.is_some(){return Err(ApiError::new(axum::http::StatusCode::TOO_MANY_REQUESTS,"PAGE_QUEUE_FULL","Wait for the current page"));}
                        let request=value["request_id"].as_str().filter(|id|!id.is_empty() && id.len()<=64).ok_or_else(ApiError::invalid)?;
                        let _=request;app.runtime.snapshot(scope,&owner,uuid(&value,"execution_id")?)?;
                        pending_page=Some(value);
                    }
                    Some("heartbeat")=> {
                        let ids=value["sessions"].as_array().filter(|ids|ids.len()<=8).ok_or_else(ApiError::invalid)?;
                        for id in ids {let id=id.as_str().and_then(|id|Uuid::parse_str(id).ok()).ok_or_else(ApiError::invalid)?;app.runtime.renew(scope,&owner,id)?;}
                    }
                    _=>return Err(ApiError::invalid()),
                }
            }
            _=interval.tick()=> {
                authorized(&app,scope,&owner).await?;
                if let Some((_,sent,id))=outstanding {
                    if sent.elapsed()>Duration::from_secs(30) {let _=app.runtime.cancel(scope,&owner,id).await;return Err(ApiError::new(axum::http::StatusCode::REQUEST_TIMEOUT,"ACK_TIMEOUT","Stream acknowledgement timed out"));}
                    continue;
                }
                if let Some(request)=pending_page.take() {
                    let execution=uuid(&request,"execution_id")?;let result=uuid(&request,"result_id")?;
                    let offset=request["offset"].as_u64().ok_or_else(ApiError::invalid)?;
                    let limit=request["limit"].as_u64().filter(|limit|*limit>0 && *limit<=256).ok_or_else(ApiError::invalid)? as usize;
                    sequence=sequence.checked_add(1).ok_or_else(ApiError::invalid)?;
                    let payload=match app.runtime.page(scope,&owner,execution,result,offset,limit) {
                        Ok(rows)=>json!({"type":"page","sequence":sequence,"request_id":request["request_id"],"execution_id":execution,"result_id":result,"rows":rows}),
                        Err(error)=>json!({"type":"page_error","sequence":sequence,"request_id":request["request_id"],"message":error.message,"code":error.code}),
                    };
                    send(&mut socket,payload).await?;outstanding=Some((sequence,Instant::now(),execution));continue;
                }
                let mut ids=watches.keys().copied().collect::<Vec<_>>();ids.sort();
                for id in ids {
                    let info=match app.runtime.snapshot(scope,&owner,id){Ok(info)=>info,Err(_)=>{watches.remove(&id);continue;}};
                    let rows=info.results.iter().map(|result|result.rows).sum::<u64>();
                    if watches.get(&id)!=Some(&(info.status,rows)) {
                        sequence=sequence.checked_add(1).ok_or_else(ApiError::invalid)?;
                        send(&mut socket,json!({"type":"notification","sequence":sequence,"execution_id":id,"status":info.status,"rows":rows})).await?;
                        watches.insert(id,(info.status,rows));outstanding=Some((sequence,Instant::now(),id));break;
                    }
                }
            }
        }
    }
    Ok(())
}
