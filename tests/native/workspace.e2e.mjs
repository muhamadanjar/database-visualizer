import { browser, $, expect } from '@wdio/globals';
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
mkdirSync('test-results/native', { recursive: true });
const request = (operation, args = {}) =>
  browser.tauri.execute(
    async ({ core }, operation, args) =>
      core.invoke('desktop_request', { request: { operation, args } }),
    operation,
    args,
  );
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
      await request('delete_profile', {
        connection_id: profile.id,
        force: true,
      });
    });
});
