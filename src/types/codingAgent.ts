export type CodingAgentId = 'claude' | 'codex' | 'opencode' | 'pi';
export type CodingAgentStartMode = 'new' | 'continue_last' | 'resume_picker';
export type CodingAgentAccessMode = 'default' | 'read_only' | 'auto_edit';

export interface CodingAgentTool {
  id: CodingAgentId;
  name: string;
  installed: boolean;
  executablePath: string | null;
  startModes: CodingAgentStartMode[];
  accessModes: CodingAgentAccessMode[];
}

export interface CodingAgentLaunchRequest {
  agentId: CodingAgentId;
  cwd: string;
  prompt?: string;
  startMode: CodingAgentStartMode;
  accessMode: CodingAgentAccessMode;
  cols?: number;
  rows?: number;
}

export type GitFileKind =
  | 'added'
  | 'modified'
  | 'deleted'
  | 'renamed'
  | 'untracked'
  | 'conflicted';

export interface GitWorkspaceFile {
  path: string;
  oldPath: string | null;
  kind: GitFileKind;
  staged: boolean;
  unstaged: boolean;
}

export interface GitWorkspaceStatus {
  root: string;
  branch: string | null;
  files: GitWorkspaceFile[];
}

export interface GitWorkspaceDiff {
  path: string;
  oldPath: string | null;
  content: string;
  truncated: boolean;
}
