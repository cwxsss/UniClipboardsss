'use strict';

const loginView = document.querySelector('#loginView');
const appView = document.querySelector('#appView');
const message = document.querySelector('#message');
const statusText = document.querySelector('#statusText');
const devicesEl = document.querySelector('#devices');
const resultPanel = document.querySelector('#resultPanel');

async function api(path, options = {}) {
  const response = await fetch(path, {
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json', ...(options.headers || {}) },
    ...options,
  });
  const payload = await response.json().catch(() => ({
    ok: false,
    error: { message: '响应不是 JSON' },
  }));
  if (!response.ok || !payload.ok) {
    const error = new Error(payload.error?.message || `HTTP ${response.status}`);
    error.status = response.status;
    throw error;
  }
  return payload.data;
}

function showMessage(text, kind = 'error') {
  message.textContent = text;
  message.className = `message ${kind}`;
}

function hideMessage() {
  message.className = 'message hidden';
  message.textContent = '';
}

function showApp() {
  loginView.classList.add('hidden');
  appView.classList.remove('hidden');
}

function showLogin() {
  appView.classList.add('hidden');
  loginView.classList.remove('hidden');
}

function formatTime(ms) {
  if (!ms) return '从未';
  return new Date(ms).toLocaleString();
}

function setRegistrationResult(data) {
  resultPanel.classList.remove('hidden');
  document.querySelector('#connectQr').src = `data:image/png;base64,${data.qrCodePngBase64}`;
  document.querySelector('#installQr').src = `data:image/png;base64,${data.installQrCodePngBase64}`;
  document.querySelector('#baseUrl').textContent = data.baseUrl || '';
  document.querySelector('#username').textContent = data.username || '';
  document.querySelector('#password').textContent = data.password || '';
}

function renderDevices(devices) {
  document.querySelector('#deviceCount').textContent = `${devices.length} 台`;
  if (devices.length === 0) {
    devicesEl.innerHTML = '<p class="empty">暂无设备</p>';
    return;
  }
  devicesEl.innerHTML = devices.map(device => `
    <article class="device">
      <div class="device-main">
        <h3>${escapeHtml(device.label || '未命名设备')}</h3>
        <p>${escapeHtml(device.username || '')}</p>
        <p class="meta">${escapeHtml(device.deviceId || '')}</p>
      </div>
      <div class="device-meta">
        <span>创建: ${escapeHtml(formatTime(device.createdAtMs))}</span>
        <span>最后访问: ${escapeHtml(formatTime(device.lastSeenAtMs))}</span>
        <span>最后 IP: ${escapeHtml(device.lastSeenIp || '-')}</span>
      </div>
      <div class="device-actions">
        <button type="button" data-rotate="${escapeAttr(device.deviceId)}">重置密码</button>
        <button type="button" class="danger" data-revoke="${escapeAttr(device.deviceId)}">删除</button>
      </div>
    </article>
  `).join('');
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, char => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[char]));
}

function escapeAttr(value) {
  return escapeHtml(value || '');
}

async function refresh() {
  hideMessage();
  const status = await api('/api/status');
  statusText.textContent = `移动同步: ${status.enabled ? '已启用' : '未启用'} | 监听: ${status.listenUrl || '-'} | 管理端口: ${status.adminPort}`;
  renderDevices(status.devices || []);
}

document.querySelector('#loginForm').addEventListener('submit', async event => {
  event.preventDefault();
  hideMessage();
  try {
    await api('/api/login', {
      method: 'POST',
      body: JSON.stringify({ password: document.querySelector('#adminPassword').value }),
    });
    showApp();
    await refresh();
  } catch (err) {
    showMessage(err.message || '登录失败');
  }
});

document.querySelector('#logoutButton').addEventListener('click', async () => {
  await api('/api/logout', { method: 'POST' }).catch(() => undefined);
  showLogin();
});

document.querySelector('#refreshButton').addEventListener('click', () => {
  refresh().catch(err => showMessage(err.message));
});

document.querySelector('#createForm').addEventListener('submit', async event => {
  event.preventDefault();
  hideMessage();
  const body = {
    label: document.querySelector('#deviceLabel').value,
    username: document.querySelector('#deviceUsername').value,
    password: document.querySelector('#devicePassword').value,
  };
  if (!body.username) delete body.username;
  if (!body.password) delete body.password;
  try {
    const result = await api('/api/devices', {
      method: 'POST',
      body: JSON.stringify(body),
    });
    setRegistrationResult(result);
    await refresh();
  } catch (err) {
    showMessage(err.message);
  }
});

document.body.addEventListener('click', async event => {
  const copyId = event.target.getAttribute('data-copy');
  if (copyId) {
    const text = document.querySelector(`#${copyId}`)?.textContent || '';
    await navigator.clipboard.writeText(text);
    showMessage('已复制', 'success');
    return;
  }

  const revokeId = event.target.getAttribute('data-revoke');
  if (revokeId) {
    if (!confirm('删除后该设备会立即失效，继续吗？')) return;
    try {
      await api(`/api/devices/${encodeURIComponent(revokeId)}`, { method: 'DELETE' });
      await refresh();
    } catch (err) {
      showMessage(err.message);
    }
    return;
  }

  const rotateId = event.target.getAttribute('data-rotate');
  if (rotateId) {
    try {
      const result = await api(`/api/devices/${encodeURIComponent(rotateId)}/rotate-password`, {
        method: 'POST',
        body: JSON.stringify({}),
      });
      resultPanel.classList.remove('hidden');
      document.querySelector('#connectQr').removeAttribute('src');
      document.querySelector('#installQr').removeAttribute('src');
      document.querySelector('#baseUrl').textContent = '';
      document.querySelector('#username').textContent = result.username || '';
      document.querySelector('#password').textContent = result.password || '';
      showMessage('新密码已生成，只显示一次。', 'success');
    } catch (err) {
      showMessage(err.message);
    }
  }
});

refresh()
  .then(showApp)
  .catch(err => {
    if (err.status === 401) showLogin();
    else showMessage(err.message);
  });
