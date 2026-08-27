import type { ComponentType } from 'react';
import type { PluginRecord } from '../../plugins/types';
import { DockerDashboard } from './DockerDashboard';
import { DatabaseConsole } from './DatabaseConsole';
import { RedisConsole } from './RedisConsole';
import { CronDashboard } from './CronDashboard';
import { ProcessDashboard } from './ProcessDashboard';
import { ServicesDashboard } from './ServicesDashboard';
import { NetworkDashboard } from './NetworkDashboard';
import { DiskDashboard } from './DiskDashboard';
import { LogsDashboard } from './LogsDashboard';
import { GitDashboard } from './GitDashboard';
import { KubeDashboard } from './KubeDashboard';

export interface DashboardProps {
  plugin: PluginRecord;
  sessionId: string;
}

/**
 * Built-in management dashboards. They orchestrate the plugin's declared
 * actions behind task-focused UIs so users never fill command-style forms for
 * the common flows. Every management plugin has one.
 */
const dashboards: Record<string, ComponentType<DashboardProps>> = {
  'docker-containers': DockerDashboard,
  'database-inspector': DatabaseConsole,
  'redis-inspector': RedisConsole,
  'cron-scheduler': CronDashboard,
  'process-explorer': ProcessDashboard,
  'systemd-services': ServicesDashboard,
  'network-inspector': NetworkDashboard,
  'disk-usage': DiskDashboard,
  'system-logs': LogsDashboard,
  'git-workspace': GitDashboard,
  'kubernetes-pods': KubeDashboard,
};

export function hasPluginDashboard(pluginId: string): boolean {
  return pluginId in dashboards;
}

export function PluginDashboard({ plugin, sessionId }: DashboardProps) {
  const Dashboard = dashboards[plugin.manifest.id];
  if (!Dashboard) return null;
  return <Dashboard plugin={plugin} sessionId={sessionId} />;
}
