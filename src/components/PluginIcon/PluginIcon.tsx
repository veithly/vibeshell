import type { LucideIcon } from 'lucide-react';
import {
  Activity,
  Box,
  Boxes,
  Clock,
  Cpu,
  Database,
  DatabaseZap,
  FileText,
  GitBranch,
  HardDrive,
  Network,
  Plug,
  Settings,
  Wrench,
} from 'lucide-react';

const icons: Record<string, LucideIcon> = {
  activity: Activity,
  box: Box,
  boxes: Boxes,
  clock: Clock,
  cpu: Cpu,
  database: Database,
  'file-text': FileText,
  'git-branch': GitBranch,
  'hard-drive': HardDrive,
  network: Network,
  plug: Plug,
  redis: DatabaseZap,
  settings: Settings,
  wrench: Wrench,
};

interface PluginIconProps {
  name: string;
  className?: string;
}

export function PluginIcon({ name, className }: PluginIconProps) {
  const Icon = icons[name] ?? Plug;
  return <Icon className={className} aria-hidden="true" />;
}
