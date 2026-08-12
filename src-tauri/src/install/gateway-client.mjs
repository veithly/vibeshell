#!/usr/bin/env node
// Small, dependency-free VibeShell Gateway client installed with the skill.
// It keeps manifest/token handling local and emits only MCP results.
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';

const appName = 'VibeShell';
const timeoutMs = 15000;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const installedManifest = {{MANIFEST_PATH_JSON}};

function manifestCandidates() {
  const home = os.homedir();
  if (process.platform === 'darwin') return [installedManifest, path.join(home, 'Library/Application Support/com.vibeshell.VibeShell/agent-gateway.json')];
  if (process.platform === 'win32') return [installedManifest, path.join(process.env.LOCALAPPDATA || path.join(home, 'AppData/Local'), 'vibeshell/agent-gateway.json')];
  return [installedManifest, path.join(process.env.XDG_DATA_HOME || path.join(home, '.local/share'), 'vibeshell/agent-gateway.json')];
}

async function readManifest() {
  for (const file of manifestCandidates()) {
    try { return JSON.parse(await fs.readFile(file, 'utf8')); } catch { /* try next */ }
  }
  return null;
}

function launch(manifest) {
  if (process.platform === 'darwin') spawn('/usr/bin/open', ['-a', appName], { detached: true, stdio: 'ignore' }).unref();
  else if (manifest?.launchPath) spawn(manifest.launchPath, [], { detached: true, stdio: 'ignore' }).unref();
  else if (process.platform === 'win32') spawn('powershell.exe', ['-NoProfile', '-Command', 'Start-Process VibeShell'], { detached: true, stdio: 'ignore' }).unref();
  else spawn('vibeshell', [], { detached: true, stdio: 'ignore' }).unref();
}

async function ready() {
  const deadline = Date.now() + timeoutMs;
  let manifest = await readManifest();
  let launched = false;
  while (Date.now() < deadline) {
    if (manifest?.status === 'running' && manifest.endpoint && manifest.token) {
      try {
        const response = await fetch(`${manifest.endpoint}/health`, { headers: { authorization: `Bearer ${manifest.token}` } });
        if (response.ok) return manifest;
      } catch { /* app may still be starting */ }
      if (!launched) { launch(manifest); launched = true; }
    } else if (!launched) { launch(manifest); launched = true; }
    await sleep(250);
    manifest = await readManifest();
  }
  throw new Error('VibeShell Gateway is not ready');
}

async function rpc(manifest, method, params, id) {
  const response = await fetch(`${manifest.endpoint}/mcp`, {
    method: 'POST',
    headers: { authorization: `Bearer ${manifest.token}`, 'content-type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id, method, params }),
  });
  if (response.status === 401) throw Object.assign(new Error('Gateway restarted; retrying'), { retry: true });
  if (!response.ok) throw new Error(`Gateway HTTP ${response.status}`);
  const payload = await response.json();
  if (payload.error) throw new Error(payload.error.message || 'MCP request failed');
  return payload.result;
}

async function call(method, params = {}) {
  let manifest = await ready();
  try {
    await rpc(manifest, 'initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'vibeshell-skill', version: '1' } }, 1);
    await rpc(manifest, 'notifications/initialized', {}, 2);
    return await rpc(manifest, method, params, 3);
  } catch (error) {
    if (error.retry) {
      manifest = await ready();
      await rpc(manifest, 'initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'vibeshell-skill', version: '1' } }, 1);
      return await rpc(manifest, method, params, 3);
    }
    throw error;
  }
}

function printResult(result) {
  const text = result?.content?.find((item) => item.type === 'text')?.text;
  if (result?.isError) throw new Error(text || 'VibeShell operation failed');
  process.stdout.write(text ?? JSON.stringify(result ?? {}));
  process.stdout.write('\n');
}

const [command, ...args] = process.argv.slice(2);
try {
  if (command === 'list') printResult(await call('tools/call', { name: 'server_list', arguments: {} }));
  else if (command === 'connect') {
    const reference = args.join(' ');
    const key = /^[0-9a-f-]{4,}$/i.test(reference) ? 'server_id' : 'server_name';
    printResult(await call('tools/call', { name: 'session_create', arguments: { [key]: reference } }));
  }
  else if (command === 'send') printResult(await call('tools/call', { name: 'session_send_input', arguments: { session_id: args[0], data: args.slice(1).join(' '), append_enter: true } }));
  else if (command === 'read') printResult(await call('tools/call', { name: 'session_read', arguments: { session_id: args[0] } }));
  else if (command === 'call') printResult(await call('tools/call', { name: args[0], arguments: args[1] ? JSON.parse(args[1]) : {} }));
  else throw new Error('Usage: node gateway.mjs list|connect <server>|send <session> <command>|read <session>|call <tool> <json>');
} catch (error) {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 1;
}
