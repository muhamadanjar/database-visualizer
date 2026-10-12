import path from 'node:path';
export const config = {
  runner: 'local',
  specs: ['./tests/native/**/*.e2e.mjs'],
  maxInstances: 1,
  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': {
        application:
          process.env.DATABASE_STUDIO_BINARY ??
          path.resolve(
            'target/debug/database-studio-desktop' +
              (process.platform === 'win32' ? '.exe' : ''),
          ),
      },
      'wdio:tauriServiceOptions': { driverProvider: 'embedded' },
    },
  ],
  services: [['@wdio/tauri-service', { driverProvider: 'embedded' }]],
  framework: 'mocha',
  reporters: ['spec'],
  logLevel: 'warn',
  waitforTimeout: 15000,
  connectionRetryTimeout: 30000,
  mochaOpts: { timeout: 120000 },
};
