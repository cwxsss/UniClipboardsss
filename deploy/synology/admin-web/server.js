#!/usr/bin/env node
'use strict';

const crypto = require('crypto');
const fs = require('fs');
const http = require('http');
const path = require('path');
const { spawn } = require('child_process');

const ROUTES = [
  'POST /api/login',
  'POST /api/logout',
  'GET /api/status',
  'GET /api/devices',
  'POST /api/devices',
  'DELETE /api/devices/',
  'POST /api/devices/{deviceId}/rotate-password',
  'POST /api/desktop-invite',
];

const port = parseInt(process.env.UC_ADMIN_PORT || '42888', 10);
const password = process.env.UC_ADMIN_PASSWORD || '';
const publicUrl = process.env.UC_MOBILE_PUBLIC_URL || '';
const uniclipBin = process.env.UC_UNICLIP_BIN || 'uniclip';
const spacePassphrase = process.env.UC_SPACE_PASSPHRASE || '';
const showSpacePassphrase = truthy(process.env.UC_ADMIN_SHOW_SPACE_PASSPHRASE || '0');
const staticDir = path.join(__dirname, 'static');
const sessions = new Map();
const sessionTtlMs = 12 * 60 * 60 * 1000;
let activeDesktopInvite = null;

if (!password) {
  console.error('UC_ADMIN_WEB=1 requires UC_ADMIN_PASSWORD');
  process.exit(1);
}

function truthy(value) {
  return /^(1|true|yes|on)$/i.test(String(value || '').trim());
}

function json(res, status, body, headers = {}) {
  res.writeHead(status, {
    'Content-Type': 'application/json; charset=utf-8',
    'Cache-Control': 'no-store',
    ...headers,
  });
  res.end(JSON.stringify(body));
}

function ok(res, data, headers) {
  json(res, 200, { ok: true, data }, headers);
}

function fail(res, status, code, message) {
  json(res, status, { ok: false, error: { code, message } });
}

function parseCookies(req) {
  const raw = req.headers.cookie || '';
  const out = {};
  for (const part of raw.split(';')) {
    const index = part.indexOf('=');
    if (index < 0) continue;
    const key = part.slice(0, index).trim();
    const value = part.slice(index + 1).trim();
    if (key) out[key] = decodeURIComponent(value);
  }
  return out;
}

function cleanupSessions(now = Date.now()) {
  for (const [id, createdAt] of sessions.entries()) {
    if (now - createdAt > sessionTtlMs) sessions.delete(id);
  }
}

function hasSession(req) {
  cleanupSessions();
  const id = parseCookies(req).uc_admin_session;
  return Boolean(id && sessions.has(id));
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    let raw = '';
    req.setEncoding('utf8');
    req.on('data', chunk => {
      raw += chunk;
      if (raw.length > 1024 * 1024) {
        reject(new Error('request body too large'));
        req.destroy();
      }
    });
    req.on('end', () => {
      if (!raw) {
        resolve({});
        return;
      }
      try {
        resolve(JSON.parse(raw));
      } catch {
        reject(new Error('invalid json'));
      }
    });
    req.on('error', reject);
  });
}

function safePasswordEquals(input) {
  const a = Buffer.from(String(input || ''));
  const b = Buffer.from(password);
  if (a.length !== b.length) return false;
  return crypto.timingSafeEqual(a, b);
}

function runUniclip(args, stdinText) {
  return new Promise((resolve, reject) => {
    const child = spawn(uniclipBin, args, {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: process.env,
      shell: process.platform === 'win32',
    });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', chunk => {
      stdout += chunk;
    });
    child.stderr.on('data', chunk => {
      stderr += chunk;
    });
    child.on('error', reject);
    child.on('close', code => {
      if (code !== 0) {
        const message = stderr.trim() || `uniclip exited with code ${code}`;
        reject(new Error(message));
        return;
      }
      try {
        resolve(JSON.parse(stdout));
      } catch {
        reject(new Error('uniclip returned invalid json'));
      }
    });
    if (stdinText != null) {
      child.stdin.end(`${stdinText}\n`);
    } else {
      child.stdin.end();
    }
  });
}

function cleanupDesktopInvite() {
  if (!activeDesktopInvite) return;
  const { child } = activeDesktopInvite;
  activeDesktopInvite = null;
  if (child.exitCode == null && !child.killed) {
    child.kill('SIGTERM');
  }
}

function shellQuote(value) {
  if (/^[A-Za-z0-9_@%+=:,./-]+$/.test(value)) return value;
  return `'${String(value).replace(/'/g, `'\\''`)}'`;
}

function buildJoinCommand(code, passphrase) {
  const renderedPassphrase = passphrase || '<你的空间口令>';
  return `uniclip join --code ${shellQuote(code)} --passphrase ${shellQuote(renderedPassphrase)}`;
}

function startDesktopInvite() {
  cleanupDesktopInvite();

  return new Promise((resolve, reject) => {
    const child = spawn(uniclipBin, ['invite'], {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: process.env,
      shell: process.platform === 'win32',
    });
    let stdout = '';
    let stderr = '';
    let settled = false;
    let code = '';

    const timeout = setTimeout(() => {
      if (settled) return;
      settled = true;
      cleanupDesktopInvite();
      reject(new Error('timed out waiting for invitation code'));
    }, 30000);

    activeDesktopInvite = { child, startedAt: Date.now() };

    function finishWithCode(nextCode) {
      if (settled) return;
      settled = true;
      code = nextCode;
      clearTimeout(timeout);
      const includePassphrase = showSpacePassphrase && Boolean(spacePassphrase);
      resolve({
        code,
        expiresAtMs: null,
        passphraseIncluded: includePassphrase,
        passphrase: includePassphrase ? spacePassphrase : '',
        command: buildJoinCommand(code, includePassphrase ? spacePassphrase : ''),
      });
    }

    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', chunk => {
      stdout += chunk;
      const match = stdout.match(/INVITATION_CODE=([^\r\n]+)/);
      if (match) finishWithCode(match[1].trim());
    });
    child.stderr.on('data', chunk => {
      stderr += chunk;
    });
    child.on('error', err => {
      if (!settled) {
        settled = true;
        clearTimeout(timeout);
        activeDesktopInvite = null;
        reject(err);
      }
    });
    child.on('close', exitCode => {
      clearTimeout(timeout);
      if (activeDesktopInvite && activeDesktopInvite.child === child) {
        activeDesktopInvite = null;
      }
      if (!settled) {
        settled = true;
        reject(new Error(stderr.trim() || `uniclip invite exited with code ${exitCode}`));
      }
    });
  });
}

function normalizeDevice(device) {
  return {
    deviceId: device.device_id || device.deviceId,
    label: device.label,
    clientType: device.client_type || device.clientType,
    username: device.username,
    createdAtMs: device.created_at_ms || device.createdAtMs || null,
    lastSeenAtMs: device.last_seen_at_ms || device.lastSeenAtMs || null,
    lastSeenIp: device.last_seen_ip || device.lastSeenIp || null,
    reportedName: device.reported_name || device.reportedName || null,
    reportedOs: device.reported_os || device.reportedOs || null,
  };
}

function normalizeRegistration(out) {
  return {
    deviceId: out.device_id || out.deviceId,
    label: out.label,
    baseUrl: out.base_url || out.baseUrl,
    username: out.username,
    password: out.password,
    installUrl: out.install_url || out.installUrl,
    installQrCodePngBase64: out.install_qr_code_png_base64 || out.installQrCodePngBase64,
    connectUri: out.connect_uri || out.connectUri,
    qrCodePngBase64: out.qr_code_png_base64 || out.qrCodePngBase64,
  };
}

async function mobileStatus() {
  const status = await runUniclip(['--json', 'mobile', 'status']);
  return {
    enabled: Boolean(status.enabled),
    lanListenEnabled: Boolean(status.lan_listen_enabled || status.lanListenEnabled),
    lanAdvertiseIp: status.lan_advertise_ip || status.lanAdvertiseIp || null,
    lanAdvertiseBaseUrl: status.lan_advertise_base_url || status.lanAdvertiseBaseUrl || null,
    lanPort: status.lan_port || status.lanPort || null,
    lanListenerError: status.lan_listener_error || status.lanListenerError || null,
    listenUrl: status.listen_url || status.listenUrl || publicUrl || '',
    adminPort: port,
    publicUrl,
    deviceCount: status.device_count || status.deviceCount || 0,
    devices: Array.isArray(status.devices) ? status.devices.map(normalizeDevice) : [],
  };
}

async function handleApi(req, res, url) {
  if (req.method === 'POST' && url.pathname === '/api/login') {
    let body;
    try {
      body = await readBody(req);
    } catch (err) {
      fail(res, 400, 'BAD_REQUEST', err.message);
      return;
    }
    if (!safePasswordEquals(body.password)) {
      fail(res, 401, 'UNAUTHORIZED', 'invalid password');
      return;
    }
    const sessionId = crypto.randomBytes(32).toString('base64url');
    sessions.set(sessionId, Date.now());
    ok(res, { authenticated: true }, {
      'Set-Cookie': `uc_admin_session=${encodeURIComponent(sessionId)}; Path=/; HttpOnly; SameSite=Strict`,
    });
    return;
  }

  if (!hasSession(req)) {
    fail(res, 401, 'UNAUTHORIZED', 'login required');
    return;
  }

  try {
    if (req.method === 'POST' && url.pathname === '/api/logout') {
      const id = parseCookies(req).uc_admin_session;
      if (id) sessions.delete(id);
      ok(res, { authenticated: false }, {
        'Set-Cookie': 'uc_admin_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict',
      });
      return;
    }

    if (req.method === 'GET' && url.pathname === '/api/status') {
      ok(res, await mobileStatus());
      return;
    }

    if (req.method === 'GET' && url.pathname === '/api/devices') {
      const status = await mobileStatus();
      ok(res, status.devices);
      return;
    }

    if (req.method === 'POST' && url.pathname === '/api/desktop-invite') {
      ok(res, await startDesktopInvite());
      return;
    }

    if (req.method === 'POST' && url.pathname === '/api/devices') {
      const body = await readBody(req);
      const label = String(body.label || '').trim();
      if (!label) {
        fail(res, 422, 'LABEL_REQUIRED', 'device label is required');
        return;
      }
      const args = ['--json', 'mobile', 'add', '--label', label];
      if (body.username) args.push('--username', String(body.username).trim());
      let stdinText = null;
      if (body.password) {
        args.push('--password-stdin');
        stdinText = String(body.password);
      }
      const out = await runUniclip(args, stdinText);
      ok(res, normalizeRegistration(out));
      return;
    }

    const revokeMatch = url.pathname.match(/^\/api\/devices\/([^/]+)$/);
    if (req.method === 'DELETE' && revokeMatch) {
      const deviceId = decodeURIComponent(revokeMatch[1]);
      await runUniclip(['--json', 'mobile', 'revoke', deviceId]);
      ok(res, { deviceId, revoked: true });
      return;
    }

    const rotateMatch = url.pathname.match(/^\/api\/devices\/([^/]+)\/rotate-password$/);
    if (req.method === 'POST' && rotateMatch) {
      const body = await readBody(req);
      const deviceId = decodeURIComponent(rotateMatch[1]);
      const args = ['--json', 'mobile', 'rotate-password', deviceId];
      let stdinText = null;
      if (body.password) {
        args.push('--password-stdin');
        stdinText = String(body.password);
      }
      const out = await runUniclip(args, stdinText);
      ok(res, {
        deviceId: out.device_id || out.deviceId,
        username: out.username,
        password: out.password,
      });
      return;
    }

    fail(res, 404, 'NOT_FOUND', 'not found');
  } catch (err) {
    fail(res, 502, 'DAEMON_UNAVAILABLE', err.message || 'daemon is not ready');
  }
}

function serveStatic(req, res, url) {
  let rel = url.pathname === '/' ? '/index.html' : url.pathname;
  rel = path.normalize(rel).replace(/^(\.\.[/\\])+/, '');
  const file = path.join(staticDir, rel);
  if (!file.startsWith(staticDir)) {
    res.writeHead(403);
    res.end('forbidden');
    return;
  }
  fs.readFile(file, (err, data) => {
    if (err) {
      res.writeHead(404);
      res.end('not found');
      return;
    }
    const ext = path.extname(file);
    const type = ext === '.js' ? 'text/javascript; charset=utf-8'
      : ext === '.css' ? 'text/css; charset=utf-8'
        : 'text/html; charset=utf-8';
    res.writeHead(200, {
      'Content-Type': type,
      'Cache-Control': 'no-store',
    });
    res.end(data);
  });
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host || 'localhost'}`);
  if (url.pathname.startsWith('/api/')) {
    void handleApi(req, res, url);
    return;
  }
  serveStatic(req, res, url);
});

server.listen(port, '0.0.0.0', () => {
  console.log(`UniClipboard admin web listening on ${port}; password (redacted); routes: ${ROUTES.length}`);
});

process.on('SIGINT', () => {
  cleanupDesktopInvite();
  process.exit(130);
});

process.on('SIGTERM', () => {
  cleanupDesktopInvite();
  process.exit(143);
});
