#!/usr/bin/env node
// Validate evidence from the CI workflow, not unrelated checks with similar names.
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export const REQUIRED_JOBS = [
  'Frontend Check',
  'Clippy Lint',
  'Rust Check (ubuntu-22.04)',
  'Rust Check (windows-latest)',
  'Rust Check (macos-latest)',
];

export function latestManualCiRun(payload, sha, repository) {
  if (!Array.isArray(payload.workflow_runs)) throw new Error('Invalid CI workflow response');
  const runs = payload.workflow_runs.filter(run =>
    run.head_sha === sha && run.event === 'workflow_dispatch'
    && run.path === '.github/workflows/ci.yml'
    && run.head_repository?.full_name === repository
    && run.repository?.full_name === repository);
  const latest = runs.sort((a, b) => b.id - a.id)[0];
  if (!latest) {
    throw new Error('Run CI manually on the exact release tag/commit before releasing');
  }
  if (!Number.isSafeInteger(latest.id) || latest.id <= 0) {
    throw new Error('The latest manual CI run for this commit has not completed successfully');
  }
  return latest;
}

export function selectManualCiRun(payload, sha, repository) {
  const latest = latestManualCiRun(payload, sha, repository);
  if (latest.status !== 'completed' || latest.conclusion !== 'success') {
    throw new Error('The latest manual CI run for this commit has not completed successfully');
  }
  return latest.id;
}

export function validateCiJobs(payload) {
  if (!Array.isArray(payload.jobs) || payload.total_count !== payload.jobs.length) {
    throw new Error('Incomplete CI jobs response');
  }
  for (const name of REQUIRED_JOBS) {
    const job = payload.jobs.find(candidate => candidate.name === name);
    if (job?.status !== 'completed' || job.conclusion !== 'success') {
      throw new Error(`Required manual CI job has not passed: ${name}`);
    }
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const [mode, file, sha, repository] = process.argv.slice(2);
    if (!file || !['run', 'jobs'].includes(mode) || (mode === 'run' && (!sha || !repository))) {
      throw new Error('Usage: check-release-ci.mjs run <runs.json> <sha> <owner/repo> | jobs <jobs.json>');
    }
    const payload = JSON.parse(readFileSync(file, 'utf8'));
    if (mode === 'run') console.log(selectManualCiRun(payload, sha, repository));
    else validateCiJobs(payload);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
