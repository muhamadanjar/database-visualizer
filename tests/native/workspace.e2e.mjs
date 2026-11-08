import { browser, $, expect } from '@wdio/globals';
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
mkdirSync('test-results/native', { recursive: true });
const request = (operation, args = {}) =>
  browser.tauri.execute(
    async ({ core }, operation, args) =>
      core.invoke('desktop_request', { request: { operation, args } }),
    operation,
    args,
  );
const restartCheck = process.env.STUDIO_E2E_RESTART_CHECK === '1';
const persistProfile = process.env.STUDIO_E2E_PERSIST_PROFILE === '1';

before(async () => {
  const readyFile = process.env.STUDIO_NATIVE_BENCHMARK_READY_FILE;
  if (readyFile) writeFileSync(readyFile, `${Date.now()}\n`);
  console.log(
    'CONSUMER_BACKPRESSURE',
    JSON.stringify({
      measured: false,
      limitation:
        'current WDIO Tauri invoke surface exposes core.invoke but no deterministic Channel hook for withholding ACK or closing the consumer',
    }),
  );
  if (readyFile) await browser.pause(2500);
});
// Embedded WebKit option-click does not dispatch select change; dispatch a DOM change.
const select = (label, value) =>
  browser.execute(
    (label, value) => {
      const node = [...document.querySelectorAll('[role="dialog"] label')]
        .find((node) => node.firstChild.textContent.trim() === label)
        ?.querySelector('select');
      if (!node) throw new Error('Select not found');
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      ).set.call(node, value);
      node.dispatchEvent(new Event('change', { bubbles: true }));
    },
    label,
    value,
  );
async function waitExecution(id) {
  await browser.waitUntil(
    async () =>
      ['Completed', 'Failed', 'Cancelled'].includes(
        (await request('execution', { execution_id: id })).status,
      ),
    { timeout: 30000, timeoutMsg: 'Execution did not terminate' },
  );
  return request('execution', { execution_id: id });
}
describe('Native desktop with real database adapters', () => {
  if (restartCheck) return;
  for (const engine of process.env.STUDIO_E2E_ENGINE
    ? [process.env.STUDIO_E2E_ENGINE]
    : ['Postgres', 'MySql'])
    it(`${engine}: create/test/connect/browse/execute/cancel/transaction/disconnect`, async () => {
      await $('button*=Koneksi').click();
      const dialog = await $('[role="dialog"]');
      await dialog.$('label*=Nama').$('input').setValue(`E2E ${engine}`);
      await select('Database engine', engine);
      await dialog.$('label=Database').$('input').setValue('studio_test');
      await dialog.$('label=Host').$('input').setValue('127.0.0.1');
      await dialog
        .$('label=Port')
        .$('input')
        .setValue(engine === 'Postgres' ? '55432' : '53306');
      await dialog.$('label=Username').$('input').setValue('studio');
      await dialog
        .$('label=Password')
        .$('input')
        .setValue(process.env.STUDIO_TEST_PASSWORD ?? 'studio_test');
      await select('TLS', 'disable');
      assert.equal(
        Number(await dialog.$('label=Port').$('input').getValue()),
        engine === 'Postgres' ? 55432 : 53306,
      );
      assert.equal(
        await dialog.$('label=Host').$('input').getValue(),
        '127.0.0.1',
      );
      assert.equal(
        await dialog.$('label=Username').$('input').getValue(),
        'studio',
      );
      assert.equal(
        await dialog.$('label=Database').$('input').getValue(),
        'studio_test',
      );
      assert.equal(
        await dialog.$('label=TLS').$('select').getValue(),
        'disable',
      );
      await $('button=Test koneksi').click();
      await expect($('.dialog-message')).toHaveText('Koneksi berhasil.');
      await $('button=Simpan koneksi').click();
      await dialog.waitForExist({ reverse: true });
      await $(`strong=E2E ${engine}`).waitForExist();
      await $(`strong=E2E ${engine}`).click();
      await $('button=Connect').click();
      await expect($('.statusbar')).toHaveText(
        expect.stringContaining(`E2E ${engine}`),
      );
      const profiles = await request('profiles');
      const profile = profiles.find((p) => p.name === `E2E ${engine}`);
      assert(profile);
      let metadata;
      await browser.waitUntil(async () => {
        try {
          metadata = await request('metadata', {
            connection_id: profile.id,
            request:
              engine === 'Postgres'
                ? 'Schemas'
                : { Objects: { schema: null, kind: 'Table' } },
          });
          return true;
        } catch (e) {
          if (!String(e.message ?? e).includes('capacity')) throw e;
          return false;
        }
      });
      assert(Array.isArray(metadata.objects));
      await $('button=Run').click();
      await browser.waitUntil(
        async () => (await $('.execution-status').getText()) === 'Completed',
      );
      await expect($('[role="grid"]')).toBeExisting();
      console.log(
        'VIEWPORT',
        await browser.execute(() => ({
          width: innerWidth,
          height: innerHeight,
          dpr: devicePixelRatio,
          gridHeight: document
            .querySelector('.grid-scroll')
            ?.getBoundingClientRect().height,
        })),
      );
      const series =
        engine === 'Postgres'
          ? 'SELECT generate_series(1,100000) AS id;'
          : 'SELECT a.n+10*b.n+100*c.n+1000*d.n+10000*e.n AS id FROM ' +
            ['a', 'b', 'c', 'd', 'e']
              .map(
                (alias) =>
                  '(SELECT 0 AS n UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9) ' +
                  alias,
              )
              .join(' CROSS JOIN ') +
            ';';
      await browser.execute((sql) => {
        if (!window.__DATABASE_STUDIO_SET_SQL__)
          throw new Error('E2E editor adapter missing');
        window.__DATABASE_STUDIO_SET_SQL__(sql);
      }, series);
      const gridStarted = Date.now();
      await $('button=Run script').click();
      await browser.waitUntil(
        async () => (await $('.execution-status').getText()) === 'Completed',
        { timeout: 30000 },
      );
      await expect($('.grid-tools')).toHaveText(
        expect.stringContaining('100.000 rows'),
      );
      await browser.execute(() => {
        const element = document.querySelector('.grid-scroll');
        element.scrollTop = element.scrollHeight;
        element.dispatchEvent(new Event('scroll'));
      });
      await browser.pause(500);
      await browser.waitUntil(
        async () =>
          await browser.execute(() =>
            [...document.querySelectorAll('.grid-row .row-number')].some(
              (n) => n.textContent === '100000',
            ),
          ),
      );
      const rendered = await browser.execute(
        () => document.querySelectorAll('.grid-row').length,
      );
      assert(rendered < 100, 'Grid must virtualize 100k rows');
      console.log(
        'GRID_100K',
        JSON.stringify({
          engine,
          elapsed_ms: Date.now() - gridStarted,
          rendered_rows: rendered,
        }),
      );
      await $('input[aria-label="Filter hasil diterima"]').setValue('99999');
      await $('button=Terapkan').click();
      await expect($('.grid-tools')).toHaveText(
        expect.stringContaining('1 rows'),
      );
      await $('button=Reset').click();
      await expect($('.grid-tools')).toHaveText(
        expect.stringContaining('100.000 rows'),
      );
      const session = await request('connect', { connection_id: profile.id });
      const result = await request('execute', {
        session_id: session.id,
        sql: 'SELECT 9007199254740993 AS precise; SELECT 42 AS answer;',
      });
      const info = await waitExecution(result.execution_id);
      assert.equal(info.results.length, 2);
      const rows = await request('page', {
        execution_id: result.execution_id,
        result_id: info.results[0].id,
        offset: 0,
        limit: 100,
      });
      assert.equal(rows[0][0].value, '9007199254740993');
      await request('close_execution', { execution_id: result.execution_id });
      let execution = await request('execute', {
        session_id: session.id,
        sql: 'BEGIN; SELECT 1;',
      });
      await waitExecution(execution.execution_id);
      assert.equal(
        (await request('session', { session_id: session.id })).transaction,
        'Active',
      );
      await request('close_execution', {
        execution_id: execution.execution_id,
      });
      execution = await request('execute', {
        session_id: session.id,
        sql: 'ROLLBACK;',
      });
      await waitExecution(execution.execution_id);
      assert.equal(
        (await request('session', { session_id: session.id })).transaction,
        'Idle',
      );
      await request('close_execution', {
        execution_id: execution.execution_id,
      });
      execution = await request('execute', {
        session_id: session.id,
        sql:
          engine === 'Postgres'
            ? 'SELECT pg_sleep(20); SELECT 999;'
            : 'SELECT SLEEP(20); SELECT 999;',
      });
      await browser.pause(300);
      await request('cancel', { execution_id: execution.execution_id });
      const cancelled = await waitExecution(execution.execution_id);
      assert.equal(cancelled.results.length, 1);
      await request('close_execution', {
        execution_id: execution.execution_id,
      });
      await request('disconnect', { session_id: session.id, force: true });
      const inspectorSession = await request('connect', {
        connection_id: profile.id,
      });
      const table = 'e2e_' + randomUUID().replaceAll('-', '');
      const setup = await request('execute', {
        session_id: inspectorSession.id,
        sql: `CREATE TABLE ${table}(id INTEGER PRIMARY KEY,label VARCHAR(80)); INSERT INTO ${table} VALUES (1,'first'),(2,'second');`,
      });
      assert.equal(
        (await waitExecution(setup.execution_id)).status,
        'Completed',
      );
      await request('close_execution', { execution_id: setup.execution_id });
      if (engine === 'Postgres') await $('button.schema-line*=public').click();
      await $('button*=Tables').click();
      await $(`button=${table}`).click();
      await expect($('.definition-table')).toHaveText(
        expect.stringContaining('label'),
      );
      await $('button=DDL').click();
      await expect($('.ddl')).toHaveText(
        expect.stringContaining('CREATE TABLE'),
      );
      await $('button=Data').click();
      await expect($('.grid-tools')).toHaveText(
        expect.stringContaining('2 rows'),
      );

      await $('button[aria-label="Pilih row 1"]').click();
      await $('button=Copy rows').click();
      await expect($('.grid-tools')).toHaveText(
        expect.stringContaining('Disalin.'),
      );
      await $('button[aria-label="Tab SQL baru"]').click();
      await browser.waitUntil(
        () =>
          browser.execute(() =>
            document
              .querySelector('.monaco-editor .view-lines')
              ?.textContent?.toLowerCase()
              .includes('version'),
          ),
        { timeout: 10000, timeoutMsg: 'New SQL tab did not mount' },
      );
      await browser.execute(
        (sql) => window.__DATABASE_STUDIO_SET_SQL__(sql),
        `UPDATE ${table} SET label='updated' WHERE id=1;`,
      );
      await browser.waitUntil(
        () =>
          browser.execute(() =>
            document
              .querySelector('.monaco-editor .view-lines')
              ?.textContent?.includes('UPDATE'),
          ),
        { timeout: 10000, timeoutMsg: 'SQL editor did not show the UPDATE' },
      );
      await $('button=Run script').click();
      await browser.waitUntil(
        async () =>
          (await $('.execution-summary').getText()).includes(
            'Affected rows: 1',
          ),
        {
          timeout: 30000,
          timeoutMsg: 'UPDATE did not complete with one affected row',
        },
      );
      await $('.tab-strip').$(`button=${table}`).click();
      await $('button=Data').click();
      await $('button=Refresh').click();
      await expect($('[role="grid"]')).toHaveText(
        expect.stringContaining('updated'),
      );
      const cleanup = await request('execute', {
        session_id: inspectorSession.id,
        sql: `DROP TABLE ${table}`,
      });
      assert.equal(
        (await waitExecution(cleanup.execution_id)).status,
        'Completed',
      );
      await request('close_execution', { execution_id: cleanup.execution_id });
      await request('disconnect', {
        session_id: inspectorSession.id,
        force: true,
      });
      await browser.saveScreenshot(
        `test-results/native/${engine.toLowerCase()}-workspace.png`,
      );
      if (!persistProfile) {
        await request('delete_profile', {
          connection_id: profile.id,
          force: true,
        });
      }
    });
});

describe('Native desktop restart persistence', () => {
  if (!restartCheck) return;

  for (const engine of process.env.STUDIO_E2E_ENGINE
    ? [process.env.STUDIO_E2E_ENGINE]
    : ['Postgres', 'MySql'])
    it(`${engine}: restores the persisted profile after a real app restart`, async () => {
      const name = `E2E ${engine}`;
      const profiles = await request('profiles');
      const profile = profiles.find((candidate) => candidate.name === name);
      assert(profile, `${name} was not persisted before restart`);
      console.log(
        'PROFILE_RESTART',
        JSON.stringify({ engine, profile_id: profile.id, restored: true }),
      );

      try {
        const session = await request('connect', { connection_id: profile.id });
        await request('disconnect', { session_id: session.id, force: true });
        console.log(
          'CREDENTIAL_RESTART',
          JSON.stringify({ engine, restored: true }),
        );
      } finally {
        await request('delete_profile', {
          connection_id: profile.id,
          force: true,
        });
      }
    });
});
