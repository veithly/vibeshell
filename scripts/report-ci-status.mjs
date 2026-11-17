#!/usr/bin/env node
// workflow_dispatch job checks are not eligible PR checks. Publish commit
// statuses from actual GitHub job evidence, never from a maintainer override.
import { pathToFileURL } from 'node:url';
import { latestManualCiRun, REQUIRED_JOBS } from './check-release-ci.mjs';

export async function reportCiStatuses(options, request) {
  const { phase, repository, sha, runId, runAttempt } = options;
  if (!['start', 'finish'].includes(phase)
      || !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository ?? '')
      || !/^[a-f0-9]{40}$/.test(sha ?? '')
      || !Number.isSafeInteger(runId) || runId <= 0
      || !Number.isSafeInteger(runAttempt) || runAttempt <= 0) {
    throw new Error('Invalid CI status reporting context');
  }
  const base = `/repos/${repository}`;
  const runUrl = `https://github.com/${repository}/actions/runs/${runId}`;
  const isCurrent = async () => {
    const payload = await request('GET', `${base}/actions/workflows/ci.yml/runs?event=workflow_dispatch&head_sha=${sha}&per_page=100`);
    const latest = latestManualCiRun(payload, sha, repository);
    if (latest.id !== runId) return false;
    const run = await request('GET', `${base}/actions/runs/${runId}`);
    return run.run_attempt === runAttempt && run.head_sha === sha && run.conclusion !== 'cancelled'
      && run.event === 'workflow_dispatch' && run.path === '.github/workflows/ci.yml'
      && run.repository?.full_name === repository
      && run.head_repository?.full_name === repository;
  };
  if (!await isCurrent()) return { superseded: true, statuses: [] };

  let jobs = [];
  if (phase === 'finish') {
    const payload = await request('GET', `${base}/actions/runs/${runId}/jobs?filter=latest&per_page=100`);
    if (!Array.isArray(payload.jobs) || payload.total_count !== payload.jobs.length) {
      throw new Error('Incomplete job evidence; refusing to publish CI success');
    }
    jobs = payload.jobs;
  }
  const statuses = REQUIRED_JOBS.map(name => {
    const matches = jobs.filter(job => job.name === name);
    const job = matches.length === 1 ? matches[0] : undefined;
    const passed = job?.status === 'completed' && job.conclusion === 'success';
    const state = phase === 'start' ? 'pending' : passed ? 'success' : 'failure';
    return {
      context: name, state, target_url: runUrl,
      description: `Manual CI #${runId}, attempt ${runAttempt}: ${phase === 'start' ? 'running' : job?.conclusion ?? 'missing job'}`,
    };
  });
  // A superseded or cancelled run must not overwrite a newer run's statuses.
  // Recheck before each write; GitHub's concurrency group cancels older runs.
  for (const status of statuses) {
    if (!await isCurrent()) return { superseded: true, statuses: [] };
    await request('POST', `${base}/statuses/${sha}`, status);
  }
  if (phase === 'finish' && statuses.some(status => status.state !== 'success')) {
    throw new Error('Required manual CI jobs did not all succeed; see the linked run');
  }
  return { superseded: false, statuses };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const token = process.env.GITHUB_TOKEN;
    if (!token) throw new Error('GITHUB_TOKEN is required for CI status reporting');
    const request = async (method, path, body) => {
      const response = await fetch(`https://api.github.com${path}`, {
        method,
        headers: {
          Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json',
          'X-GitHub-Api-Version': '2022-11-28', 'Content-Type': 'application/json',
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(15000),
      });
      // Do not echo request headers or API error bodies into workflow logs.
      if (!response.ok) throw new Error(`GitHub CI status API failed (${response.status})`);
      return response.json();
    };
    const result = await reportCiStatuses({
      phase: process.argv[2], repository: process.env.GITHUB_REPOSITORY,
      sha: process.env.GITHUB_SHA, runId: Number(process.env.GITHUB_RUN_ID),
      runAttempt: Number(process.env.GITHUB_RUN_ATTEMPT),
    }, request);
    console.log(result.superseded ? 'Inactive or superseded CI attempt; no further status updates.'
      : `Published ${result.statuses.length} evidence-backed ${process.argv[2]} statuses.`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
