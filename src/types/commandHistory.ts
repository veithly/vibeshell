/** A command previously executed against a configured SSH server. */
export interface CommandHistoryEntry {
  id: string;
  server_id: string;
  command: string;
  is_favorite: boolean;
  use_count: number;
  last_used_at: number;
  created_at: number;
}
