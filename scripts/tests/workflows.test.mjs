import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { REQUIRED_JOBS, selectManualCiRun, validateCiJobs } from '../check-release-ci.mjs';

const sha = 'a'.repeat(40);
const repository = 'owner/repo';
const run = {
  id: 42, head_sha: sha, event: 'workflow_dispatch', path: '.github/workflows/ci.yml',
  head_repository: { full_name: repository }, repository: { full_name: repository },
  status: 'completed', conclusion: 'success',
};
const select = runs => selectManualCiRun({ workflow_runs: runs }, sha, repository);
const jobs = () => ({
  total_count: REQUIRED_JOBS.length,
  jobs: REQUIRED_JOBS.map(name => ({ name, status: 'completed', conclusion: 'success' })),
});

test('selects a successful manual CI run for the exact repository and commit', () => {
  assert.equal(select([run]), 42);
});

test('does not accept automatic, foreign, wrong-workflow or wrong-commit evidence', () => {
  for (const override of [
    { event: 'push' }, { head_sha: 'b'.repeat(40) }, { path: '.github/workflows/other.yml' },
    { head_repository: { full_name: 'fork/repo' } }, { repository: { full_name: 'fork/repo' } },
  ]) assert.throws(() => select([{ ...run, ...override }]), /Run CI manually/);
  assert.throws(() => select([]), /Run CI manually/);
});

test('an older success cannot hide a newer failed, cancelled or running attempt', () => {
  for (const override of [
    { conclusion: 'failure' }, { conclusion: 'cancelled' }, { conclusion: 'skipped' },
    { status: 'in_progress', conclusion: null },
  ]) assert.throws(() => select([run, { ...run, id: 43, ...override }]), /latest manual CI/);
});

test('selects the newest successful run independent of response order', () => {
  assert.equal(select([{ ...run, id: 43 }, run]), 43);
});

test('requires every platform, frontend and lint job to have really succeeded', () => {
  assert.doesNotThrow(() => validateCiJobs(jobs()));
  for (const conclusion of ['failure', 'cancelled', 'skipped', null]) {
    const payload = jobs();
    payload.jobs[0].conclusion = conclusion;
    assert.throws(() => validateCiJobs(payload), /Required manual CI job/);
  }
  const payload = jobs();
  payload.jobs.pop();
  payload.total_count--;
  assert.throws(() => validateCiJobs(payload), /Rust Check/);
});

test('rejects malformed or incomplete evidence', () => {
  assert.throws(() => selectManualCiRun({}, sha, repository), /Invalid CI/);
  assert.throws(() => select([{ ...run, id: -1 }]), /latest manual CI/);
  const payload = jobs();
  payload.total_count++;
  assert.throws(() => validateCiJobs(payload), /Incomplete CI/);
});

// These files deliberately use a block-form, top-level `on:` section. Reject
// shape changes too, so adding an automatic trigger cannot evade this guard.
test('CI and release have only the manual trigger', () => {
  for (const file of ['ci.yml', 'release.yml']) {
    const text = readFileSync(new URL(`../../.github/workflows/${file}`, import.meta.url), 'utf8').replaceAll('\r\n', '\n');
    const section = text.match(/^on:\n((?:[ \t].*\n|\n)+)/m);
    assert.ok(section, `${file}: expected block-form on section`);
    const events = [...section[1].matchAll(/^  ([a-z_]+):/gm)].map(match => match[1]);
    assert.deepEqual(events, ['workflow_dispatch'], file);
  }
});

test('release is restricted to main and defaults to an unpublished draft', () => {
  const text = readFileSync(new URL('../../.github/workflows/release.yml', import.meta.url), 'utf8');
  assert.match(text, /if: github.event_name == 'workflow_dispatch' && github.ref == 'refs\/heads\/main'/);
  assert.match(text, /publish:[\s\S]*?type: boolean\s+default: false/);
  assert.match(text, /if \[\[ "\$PUBLISH_RELEASE" == 'true' \]\]; then\s+gh release edit[^\n]*--draft=false --latest/);
  assert.doesNotMatch(text, /inputs\.tag \|\| github\.ref_name/);
});
