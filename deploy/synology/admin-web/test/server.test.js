'use strict';

const assert = require('assert/strict');
const { EventEmitter } = require('events');
const http = require('http');
const { PassThrough } = require('stream');
const test = require('node:test');

const serverModulePath = require.resolve('../server.js');
const childProcess = require('child_process');
const originalSpawn = childProcess.spawn;

function fakeSpawn(_command, args) {
  const child = new EventEmitter();
  child.stdin = new PassThrough();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  child.exitCode = null;
  child.killed = false;
  child.kill = () => {
    child.killed = true;
    child.exitCode = 0;
    child.emit('close', 0);
    return true;
  };

  setImmediate(() => {
    if (args.includes('invite')) {
      child.stdout.end('INVITATION_CODE=server-generated-code\n');
    } else if (args.includes('status')) {
      child.stdout.end(JSON.stringify({ enabled: true, devices: ['long-output-for-limit-test'] }));
    } else {
      child.stdout.end('{}');
    }
    child.stderr.end();
    child.exitCode = 0;
    child.emit('close', 0);
  });

  return child;
}

function request(port, path, options = {}) {
  return new Promise((resolve, reject) => {
    const req = http.request({
      hostname: '127.0.0.1',
      port,
      path,
      method: options.method || 'GET',
      headers: options.headers,
    }, response => {
      let body = '';
      response.setEncoding('utf8');
      response.on('data', chunk => { body += chunk; });
      response.on('end', () => resolve({
        status: response.statusCode,
        headers: response.headers,
        body: body ? JSON.parse(body) : null,
      }));
    });
    req.on('error', reject);
    if (options.body) req.write(JSON.stringify(options.body));
    req.end();
  });
}

async function withAdminServer(environment, run) {
  const previousEnvironment = {};
  for (const [key, value] of Object.entries(environment)) {
    previousEnvironment[key] = process.env[key];
    process.env[key] = value;
  }

  childProcess.spawn = fakeSpawn;
  delete require.cache[serverModulePath];
  const { server } = require('../server.js');
  await new Promise(resolve => server.once('listening', resolve));

  try {
    await run(server.address().port);
  } finally {
    await new Promise(resolve => server.close(resolve));
    delete require.cache[serverModulePath];
    childProcess.spawn = originalSpawn;
    for (const [key, value] of Object.entries(previousEnvironment)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

async function login(port) {
  const response = await request(port, '/api/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: { password: 'test-admin-password' },
  });
  assert.equal(response.status, 200);
  return response.headers['set-cookie'][0].split(';', 1)[0];
}

test('desktop invitation ignores client supplied code and passphrase when disclosure is disabled', async () => {
  await withAdminServer({
    UC_ADMIN_PORT: '0',
    UC_ADMIN_PASSWORD: 'test-admin-password',
    UC_SPACE_PASSPHRASE: 'server-space-passphrase',
    UC_ADMIN_SHOW_SPACE_PASSPHRASE: '0',
    UC_ADMIN_COOKIE_SECURE: '0',
  }, async port => {
    const cookie = await login(port);
    const response = await request(port, '/api/desktop-invite', {
      method: 'POST',
      headers: { Cookie: cookie, 'Content-Type': 'application/json' },
      body: {
        customCode: 'client-supplied-code',
        passphrase: 'client-supplied-passphrase',
      },
    });

    assert.equal(response.status, 200);
    assert.equal(response.body.data.code, 'server-generated-code');
    assert.equal(response.body.data.passphraseIncluded, false);
    assert.equal(response.body.data.passphrase, '');
    assert.equal(response.body.data.harmonyJoinUri, '');
    assert.match(response.body.data.command, /<.+>/);
  });
});

test('desktop invitation includes the configured passphrase only when disclosure is enabled', async () => {
  await withAdminServer({
    UC_ADMIN_PORT: '0',
    UC_ADMIN_PASSWORD: 'test-admin-password',
    UC_SPACE_PASSPHRASE: 'server-space-passphrase',
    UC_ADMIN_SHOW_SPACE_PASSPHRASE: '1',
    UC_ADMIN_COOKIE_SECURE: '0',
  }, async port => {
    const cookie = await login(port);
    const response = await request(port, '/api/desktop-invite', {
      method: 'POST',
      headers: { Cookie: cookie, 'Content-Type': 'application/json' },
      body: { deviceName: 'Test desktop' },
    });

    assert.equal(response.status, 200);
    assert.equal(response.body.data.passphraseIncluded, true);
    assert.equal(response.body.data.passphrase, 'server-space-passphrase');
    assert.match(response.body.data.harmonyJoinUri, /pwd=server-space-passphrase/);
    assert.match(response.body.data.command, /--device-name 'Test desktop'/);
  });
});

test('admin session cookie adds Secure only when configured for HTTPS', async () => {
  await withAdminServer({
    UC_ADMIN_PORT: '0',
    UC_ADMIN_PASSWORD: 'test-admin-password',
    UC_SPACE_PASSPHRASE: 'server-space-passphrase',
    UC_ADMIN_SHOW_SPACE_PASSPHRASE: '0',
    UC_ADMIN_COOKIE_SECURE: '1',
  }, async port => {
    const response = await request(port, '/api/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: { password: 'test-admin-password' },
    });

    assert.equal(response.status, 200);
    assert.match(response.headers['set-cookie'][0], /; Secure$/);
  });
});

test('rejects daemon output that exceeds the configured limit', async () => {
  await withAdminServer({
    UC_ADMIN_PORT: '0',
    UC_ADMIN_PASSWORD: 'test-admin-password',
    UC_SPACE_PASSPHRASE: 'server-space-passphrase',
    UC_ADMIN_SHOW_SPACE_PASSPHRASE: '0',
    UC_ADMIN_COOKIE_SECURE: '0',
    UC_ADMIN_OUTPUT_MAX_BYTES: '16',
  }, async port => {
    const cookie = await login(port);
    const response = await request(port, '/api/status', {
      headers: { Cookie: cookie },
    });

    assert.equal(response.status, 502);
    assert.equal(response.body.error.code, 'COMMAND_OUTPUT_TOO_LARGE');
  });
});
