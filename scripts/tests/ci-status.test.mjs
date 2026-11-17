import assert from 'node:assert/strict';
import test from 'node:test';
import { reportCiStatuses } from '../report-ci-status.mjs';
import { REQUIRED_JOBS } from '../check-release-ci.mjs';

const repository = 'owner/repo';
const sha = 'a'.repeat(40);
const options = { phase: 'finish', repository, sha, runId: 42, runAttempt: 1 };
function fixture(overrides = {}) {
  const run = {
    id: 42, head_sha: sha, event: 'workflow_dispatch', path: '.github/workflows/ci.yml',
    repository: { full_name: repository }, head_repository: { full_name: repository },
    status: 'in_progress', conclusion: null, run_attempt: 1,
  };
  const jobs = REQUIRED_JOBS.map(name => ({ name, status: 'completed', conclusion: 'success' }));
  const state = { run, runs: [run], jobs, posts: [], ...overrides };
  state.request = async (method, path, body) => {
    if (method === 'POST') { state.posts.push({ path, body }); return {}; }
    if (path.includes('/jobs?')) return { jobs: state.jobs, total_count: state.total ?? state.jobs.length };
    if (path.includes('/workflows/')) return { workflow_runs: state.runs };
    if (path.endsWith('/runs/42')) return state.run;
    throw new Error(`Unexpected test endpoint: ${path}`);
  };
  return state;
}

test('initialization marks all five required contexts pending, never green', async () => {
  const state = fixture();
  await reportCiStatuses({ ...options, phase: 'start' }, state.request);
  assert.equal(state.posts.length, REQUIRED_JOBS.length);
  assert.deepEqual(state.posts.map(post => post.body.context), REQUIRED_JOBS);
  assert.ok(state.posts.every(post => post.body.state === 'pending'));
});

test('only real successful jobs publish success for the exact tested commit', async () => {
  const state = fixture();
  await reportCiStatuses(options, state.request);
  assert.equal(state.posts.length, REQUIRED_JOBS.length);
  assert.ok(state.posts.every(post => post.path === `/repos/${repository}/statuses/${sha}`));
  assert.ok(state.posts.every(post => post.body.state === 'success'));
  assert.ok(state.posts.every(post => post.body.target_url.endsWith('/actions/runs/42')));
});

test('failure, cancellation, skipping and incomplete jobs can never produce green', async () => {
  for (const conclusion of ['failure', 'cancelled', 'skipped', 'neutral', 'timed_out', null]) {
    const state = fixture();
    state.jobs[0].conclusion = conclusion;
    await assert.rejects(reportCiStatuses(options, state.request), /did not all succeed/);
    assert.equal(state.posts[0].body.state, 'failure');
  }
  const state = fixture();
  state.jobs[0].status = 'in_progress';
  await assert.rejects(reportCiStatuses(options, state.request), /did not all succeed/);
  assert.equal(state.posts[0].body.state, 'failure');
});

test('missing and duplicate required job names fail closed', async () => {
  for (const duplicate of [false, true]) {
    const state = fixture();
    if (duplicate) state.jobs.push({ ...state.jobs[0] });
    else state.jobs.shift();
    await assert.rejects(reportCiStatuses(options, state.request), /did not all succeed/);
    assert.equal(state.posts[0].body.state, 'failure');
  }
});

test('incomplete API pages cannot be treated as complete job evidence', async () => {
  const state = fixture({ total: 100 });
  await assert.rejects(reportCiStatuses(options, state.request), /Incomplete job evidence/);
  assert.equal(state.posts.length, 0);
});

test('older runs and stale rerun attempts do not overwrite current statuses', async () => {
  const state = fixture();
  state.runs.push({ ...state.run, id: 43 });
  assert.equal((await reportCiStatuses(options, state.request)).superseded, true);
  assert.equal(state.posts.length, 0);
  const rerun = fixture();
  rerun.run.run_attempt = 2;
  assert.equal((await reportCiStatuses(options, rerun.request)).superseded, true);
  assert.equal(rerun.posts.length, 0);
});

test('a run superseded while fetching evidence cannot write a stale success', async () => {
  const state = fixture();
  const request = async (...args) => {
    const response = await state.request(...args);
    if (args[1].includes('/jobs?')) state.runs.push({ ...state.run, id: 43 });
    return response;
  };
  assert.equal((await reportCiStatuses(options, request)).superseded, true);
  assert.equal(state.posts.length, 0);
});

test('a cancelled workflow cannot publish green even after its test jobs succeeded', async () => {
  const state = fixture();
  state.run.conclusion = 'cancelled';
  assert.equal((await reportCiStatuses(options, state.request)).superseded, true);
  assert.equal(state.posts.length, 0);
});

test('foreign repository, wrong SHA or automatic run evidence is rejected', async () => {
  for (const override of [
    { event: 'push' }, { head_sha: 'b'.repeat(40) },
    { head_repository: { full_name: 'fork/repo' } },
  ]) {
    const state = fixture();
    Object.assign(state.run, override);
    await assert.rejects(reportCiStatuses(options, state.request), /Run CI manually/);
    assert.equal(state.posts.length, 0);
  }
});

test('invalid reporting context is rejected before any API call', async () => {
  for (const override of [{ phase: 'force' }, { runId: 0 }, { runAttempt: 0 }, { sha: 'HEAD' }, { repository: 'bad/path/extra' }]) {
    await assert.rejects(reportCiStatuses({ ...options, ...override }, () => {
      assert.fail('Invalid context must not reach the API');
    }), /Invalid CI status/);
  }
});
