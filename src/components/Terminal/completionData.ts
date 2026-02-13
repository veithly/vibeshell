/**
 * Command completion data source for terminal autocomplete functionality.
 * Contains common shell commands, their descriptions, and subcommands.
 */

/**
 * Represents a command suggestion with metadata.
 */
export interface CommandSuggestion {
  /** The command or subcommand text */
  text: string;
  /** Optional description of what the command does */
  description?: string;
  /** Command category for grouping */
  category: CommandCategory;
  /** Subcommands available for this command */
  subcommands?: string[];
}

/**
 * Categories for organizing commands.
 */
export type CommandCategory =
  | 'navigation'
  | 'file'
  | 'text'
  | 'system'
  | 'network'
  | 'vcs'
  | 'container'
  | 'package'
  | 'misc';

/**
 * Category display information.
 */
export const categoryInfo: Record<CommandCategory, { label: string; color: string }> = {
  navigation: { label: 'Navigation', color: '#7aa2f7' },
  file: { label: 'Files', color: '#9ece6a' },
  text: { label: 'Text', color: '#e0af68' },
  system: { label: 'System', color: '#f7768e' },
  network: { label: 'Network', color: '#7dcfff' },
  vcs: { label: 'Git', color: '#ff9e64' },
  container: { label: 'Container', color: '#bb9af7' },
  package: { label: 'Package', color: '#73daca' },
  misc: { label: 'Misc', color: '#a9b1d6' },
};

/**
 * Common shell commands with descriptions and categories.
 */
export const commonCommands: CommandSuggestion[] = [
  // Navigation commands
  { text: 'cd', description: 'Change directory', category: 'navigation' },
  { text: 'ls', description: 'List directory contents', category: 'navigation', subcommands: ['-la', '-lh', '-R', '-a', '-l', '-t', '-S'] },
  { text: 'pwd', description: 'Print working directory', category: 'navigation' },
  { text: 'pushd', description: 'Push directory onto stack', category: 'navigation' },
  { text: 'popd', description: 'Pop directory from stack', category: 'navigation' },
  { text: 'dirs', description: 'Display directory stack', category: 'navigation' },

  // File operations
  { text: 'cat', description: 'Concatenate and display files', category: 'file' },
  { text: 'less', description: 'View file with pagination', category: 'file' },
  { text: 'more', description: 'View file page by page', category: 'file' },
  { text: 'head', description: 'Output first part of file', category: 'file', subcommands: ['-n', '-c'] },
  { text: 'tail', description: 'Output last part of file', category: 'file', subcommands: ['-n', '-f', '-F'] },
  { text: 'touch', description: 'Create empty file or update timestamp', category: 'file' },
  { text: 'mkdir', description: 'Create directory', category: 'file', subcommands: ['-p', '-v'] },
  { text: 'rmdir', description: 'Remove empty directory', category: 'file' },
  { text: 'rm', description: 'Remove files or directories', category: 'file', subcommands: ['-r', '-f', '-rf', '-i'] },
  { text: 'cp', description: 'Copy files or directories', category: 'file', subcommands: ['-r', '-v', '-i', '-a'] },
  { text: 'mv', description: 'Move or rename files', category: 'file', subcommands: ['-v', '-i', '-n'] },
  { text: 'ln', description: 'Create links', category: 'file', subcommands: ['-s', '-f'] },
  { text: 'chmod', description: 'Change file permissions', category: 'file', subcommands: ['+x', '-R'] },
  { text: 'chown', description: 'Change file owner', category: 'file', subcommands: ['-R'] },
  { text: 'chgrp', description: 'Change file group', category: 'file', subcommands: ['-R'] },
  { text: 'file', description: 'Determine file type', category: 'file' },
  { text: 'stat', description: 'Display file status', category: 'file' },
  { text: 'find', description: 'Search for files', category: 'file', subcommands: ['-name', '-type', '-mtime', '-size', '-exec'] },
  { text: 'locate', description: 'Find files by name', category: 'file' },
  { text: 'which', description: 'Locate a command', category: 'file' },
  { text: 'whereis', description: 'Locate binary, source, manual', category: 'file' },
  { text: 'tree', description: 'List directory tree', category: 'file', subcommands: ['-L', '-a', '-d'] },

  // Text processing
  { text: 'grep', description: 'Search text patterns', category: 'text', subcommands: ['-r', '-i', '-v', '-n', '-l', '-E', '-c'] },
  { text: 'awk', description: 'Pattern scanning and processing', category: 'text' },
  { text: 'sed', description: 'Stream editor', category: 'text', subcommands: ['-i', '-e', '-n'] },
  { text: 'sort', description: 'Sort lines of text', category: 'text', subcommands: ['-n', '-r', '-u', '-k'] },
  { text: 'uniq', description: 'Report or filter repeated lines', category: 'text', subcommands: ['-c', '-d', '-u'] },
  { text: 'cut', description: 'Remove sections from lines', category: 'text', subcommands: ['-d', '-f', '-c'] },
  { text: 'paste', description: 'Merge lines of files', category: 'text' },
  { text: 'tr', description: 'Translate characters', category: 'text' },
  { text: 'wc', description: 'Word, line, character count', category: 'text', subcommands: ['-l', '-w', '-c'] },
  { text: 'diff', description: 'Compare files line by line', category: 'text', subcommands: ['-u', '-y', '-r'] },
  { text: 'comm', description: 'Compare sorted files', category: 'text' },
  { text: 'tee', description: 'Read and write to stdout and files', category: 'text', subcommands: ['-a'] },
  { text: 'xargs', description: 'Build and execute commands', category: 'text', subcommands: ['-I', '-n', '-0'] },
  { text: 'echo', description: 'Display a line of text', category: 'text', subcommands: ['-n', '-e'] },
  { text: 'printf', description: 'Format and print data', category: 'text' },

  // System commands
  { text: 'ps', description: 'Report process status', category: 'system', subcommands: ['aux', '-ef', '-e'] },
  { text: 'top', description: 'Display processes', category: 'system' },
  { text: 'htop', description: 'Interactive process viewer', category: 'system' },
  { text: 'kill', description: 'Send signal to process', category: 'system', subcommands: ['-9', '-15', '-SIGTERM', '-SIGKILL'] },
  { text: 'killall', description: 'Kill processes by name', category: 'system' },
  { text: 'pkill', description: 'Signal processes by pattern', category: 'system' },
  { text: 'pgrep', description: 'Find processes by pattern', category: 'system' },
  { text: 'bg', description: 'Move job to background', category: 'system' },
  { text: 'fg', description: 'Move job to foreground', category: 'system' },
  { text: 'jobs', description: 'List active jobs', category: 'system' },
  { text: 'nohup', description: 'Run command immune to hangups', category: 'system' },
  { text: 'df', description: 'Report disk space usage', category: 'system', subcommands: ['-h', '-T'] },
  { text: 'du', description: 'Estimate file space usage', category: 'system', subcommands: ['-h', '-s', '-a'] },
  { text: 'free', description: 'Display memory usage', category: 'system', subcommands: ['-h', '-m', '-g'] },
  { text: 'uptime', description: 'Show system uptime', category: 'system' },
  { text: 'uname', description: 'Print system information', category: 'system', subcommands: ['-a', '-r', '-m'] },
  { text: 'hostname', description: 'Show or set hostname', category: 'system' },
  { text: 'whoami', description: 'Print current user', category: 'system' },
  { text: 'who', description: 'Show who is logged in', category: 'system' },
  { text: 'w', description: 'Show who is logged in and what they are doing', category: 'system' },
  { text: 'id', description: 'Print user and group IDs', category: 'system' },
  { text: 'groups', description: 'Print group memberships', category: 'system' },
  { text: 'sudo', description: 'Execute as superuser', category: 'system', subcommands: ['-i', '-s', '-u'] },
  { text: 'su', description: 'Switch user', category: 'system' },
  { text: 'passwd', description: 'Change password', category: 'system' },
  { text: 'useradd', description: 'Create user account', category: 'system' },
  { text: 'userdel', description: 'Delete user account', category: 'system' },
  { text: 'usermod', description: 'Modify user account', category: 'system' },
  { text: 'groupadd', description: 'Create group', category: 'system' },
  { text: 'mount', description: 'Mount filesystem', category: 'system' },
  { text: 'umount', description: 'Unmount filesystem', category: 'system' },
  { text: 'lsblk', description: 'List block devices', category: 'system' },
  { text: 'fdisk', description: 'Partition table manipulator', category: 'system', subcommands: ['-l'] },
  { text: 'systemctl', description: 'Control systemd', category: 'system', subcommands: ['start', 'stop', 'restart', 'status', 'enable', 'disable', 'list-units'] },
  { text: 'service', description: 'Run init scripts', category: 'system', subcommands: ['start', 'stop', 'restart', 'status'] },
  { text: 'journalctl', description: 'Query systemd journal', category: 'system', subcommands: ['-f', '-u', '-xe', '--since', '--until'] },
  { text: 'dmesg', description: 'Print kernel messages', category: 'system' },
  { text: 'lsof', description: 'List open files', category: 'system', subcommands: ['-i', '-p', '-u'] },
  { text: 'strace', description: 'Trace system calls', category: 'system' },
  { text: 'history', description: 'Command history', category: 'system' },
  { text: 'clear', description: 'Clear terminal screen', category: 'system' },
  { text: 'exit', description: 'Exit shell', category: 'system' },
  { text: 'logout', description: 'Logout from shell', category: 'system' },
  { text: 'reboot', description: 'Reboot system', category: 'system' },
  { text: 'shutdown', description: 'Shutdown system', category: 'system', subcommands: ['-h', '-r', 'now'] },
  { text: 'date', description: 'Display or set date/time', category: 'system' },
  { text: 'cal', description: 'Display calendar', category: 'system' },
  { text: 'timedatectl', description: 'Control time and date', category: 'system' },
  { text: 'crontab', description: 'Manage cron jobs', category: 'system', subcommands: ['-l', '-e', '-r'] },
  { text: 'env', description: 'Display environment', category: 'system' },
  { text: 'export', description: 'Set environment variable', category: 'system' },
  { text: 'alias', description: 'Create command alias', category: 'system' },
  { text: 'unalias', description: 'Remove command alias', category: 'system' },
  { text: 'source', description: 'Execute commands from file', category: 'system' },
  { text: 'man', description: 'Display manual page', category: 'system' },
  { text: 'info', description: 'Read info documents', category: 'system' },
  { text: 'help', description: 'Display help for builtin commands', category: 'system' },
  { text: 'type', description: 'Display command type', category: 'system' },

  // Network commands
  { text: 'ssh', description: 'Secure shell client', category: 'network', subcommands: ['-i', '-p', '-L', '-R', '-D', '-v'] },
  { text: 'scp', description: 'Secure copy over SSH', category: 'network', subcommands: ['-r', '-P', '-i'] },
  { text: 'sftp', description: 'Secure FTP over SSH', category: 'network' },
  { text: 'rsync', description: 'Remote file sync', category: 'network', subcommands: ['-avz', '-e', '--delete', '--progress'] },
  { text: 'curl', description: 'Transfer data from URL', category: 'network', subcommands: ['-X', '-H', '-d', '-o', '-O', '-L', '-s', '-v'] },
  { text: 'wget', description: 'Download files', category: 'network', subcommands: ['-O', '-c', '-r', '-q'] },
  { text: 'ping', description: 'Send ICMP echo requests', category: 'network', subcommands: ['-c', '-i'] },
  { text: 'traceroute', description: 'Trace packet route', category: 'network' },
  { text: 'netstat', description: 'Network statistics', category: 'network', subcommands: ['-tulpn', '-an'] },
  { text: 'ss', description: 'Socket statistics', category: 'network', subcommands: ['-tulpn', '-an'] },
  { text: 'ifconfig', description: 'Configure network interface', category: 'network' },
  { text: 'ip', description: 'Show/manipulate routing, devices', category: 'network', subcommands: ['addr', 'link', 'route', 'a', 'r'] },
  { text: 'nslookup', description: 'Query DNS', category: 'network' },
  { text: 'dig', description: 'DNS lookup utility', category: 'network' },
  { text: 'host', description: 'DNS lookup utility', category: 'network' },
  { text: 'nmap', description: 'Network exploration tool', category: 'network', subcommands: ['-sS', '-sV', '-O', '-A'] },
  { text: 'tcpdump', description: 'Dump traffic on network', category: 'network' },
  { text: 'iptables', description: 'IPv4 firewall admin', category: 'network', subcommands: ['-L', '-A', '-D', '-F'] },
  { text: 'firewall-cmd', description: 'Firewalld client', category: 'network' },
  { text: 'nc', description: 'Netcat - network swiss army knife', category: 'network', subcommands: ['-l', '-p', '-v', '-z'] },
  { text: 'telnet', description: 'Telnet client', category: 'network' },
  { text: 'ftp', description: 'FTP client', category: 'network' },

  // Git commands
  { text: 'git', description: 'Version control system', category: 'vcs', subcommands: ['init', 'clone', 'status', 'add', 'commit', 'push', 'pull', 'fetch', 'merge', 'rebase', 'branch', 'checkout', 'switch', 'log', 'diff', 'stash', 'reset', 'revert', 'tag', 'remote', 'show', 'blame', 'cherry-pick'] },

  // Container commands
  { text: 'docker', description: 'Container platform', category: 'container', subcommands: ['run', 'ps', 'images', 'build', 'pull', 'push', 'exec', 'logs', 'stop', 'start', 'rm', 'rmi', 'network', 'volume', 'compose', 'system'] },
  { text: 'docker-compose', description: 'Multi-container Docker', category: 'container', subcommands: ['up', 'down', 'ps', 'logs', 'build', 'exec', 'restart'] },
  { text: 'podman', description: 'Container management', category: 'container', subcommands: ['run', 'ps', 'images', 'build', 'pull', 'exec', 'logs', 'stop', 'rm'] },
  { text: 'kubectl', description: 'Kubernetes CLI', category: 'container', subcommands: ['get', 'describe', 'create', 'apply', 'delete', 'logs', 'exec', 'port-forward', 'scale', 'rollout', 'config', 'cluster-info'] },
  { text: 'helm', description: 'Kubernetes package manager', category: 'container', subcommands: ['install', 'upgrade', 'uninstall', 'list', 'repo', 'search', 'show'] },

  // Package managers
  { text: 'apt', description: 'Debian package manager', category: 'package', subcommands: ['update', 'upgrade', 'install', 'remove', 'purge', 'search', 'list', 'autoremove'] },
  { text: 'apt-get', description: 'Debian package manager', category: 'package', subcommands: ['update', 'upgrade', 'install', 'remove', 'purge', 'autoremove'] },
  { text: 'dpkg', description: 'Debian package tool', category: 'package', subcommands: ['-i', '-r', '-l', '-L', '-S'] },
  { text: 'yum', description: 'RPM package manager', category: 'package', subcommands: ['install', 'remove', 'update', 'search', 'list', 'info'] },
  { text: 'dnf', description: 'Fedora package manager', category: 'package', subcommands: ['install', 'remove', 'update', 'search', 'list', 'info'] },
  { text: 'rpm', description: 'RPM package tool', category: 'package', subcommands: ['-i', '-e', '-q', '-qa', '-ql'] },
  { text: 'pacman', description: 'Arch package manager', category: 'package', subcommands: ['-S', '-R', '-Syu', '-Ss', '-Q', '-Qi'] },
  { text: 'brew', description: 'Homebrew package manager', category: 'package', subcommands: ['install', 'uninstall', 'update', 'upgrade', 'search', 'list', 'info'] },
  { text: 'snap', description: 'Snap package manager', category: 'package', subcommands: ['install', 'remove', 'list', 'find', 'refresh'] },
  { text: 'flatpak', description: 'Flatpak package manager', category: 'package', subcommands: ['install', 'uninstall', 'list', 'search', 'update'] },
  { text: 'pip', description: 'Python package manager', category: 'package', subcommands: ['install', 'uninstall', 'list', 'show', 'freeze', 'search'] },
  { text: 'pip3', description: 'Python 3 package manager', category: 'package', subcommands: ['install', 'uninstall', 'list', 'show', 'freeze'] },
  { text: 'npm', description: 'Node.js package manager', category: 'package', subcommands: ['install', 'uninstall', 'run', 'start', 'test', 'build', 'init', 'publish', 'update', 'list'] },
  { text: 'yarn', description: 'JavaScript package manager', category: 'package', subcommands: ['add', 'remove', 'install', 'run', 'build', 'start', 'test'] },
  { text: 'pnpm', description: 'Fast Node.js package manager', category: 'package', subcommands: ['add', 'remove', 'install', 'run', 'build', 'start', 'test'] },
  { text: 'cargo', description: 'Rust package manager', category: 'package', subcommands: ['build', 'run', 'test', 'new', 'init', 'add', 'remove', 'publish', 'install'] },
  { text: 'gem', description: 'Ruby package manager', category: 'package', subcommands: ['install', 'uninstall', 'list', 'search', 'update'] },
  { text: 'composer', description: 'PHP package manager', category: 'package', subcommands: ['install', 'update', 'require', 'remove', 'dump-autoload'] },
  { text: 'go', description: 'Go language tool', category: 'package', subcommands: ['build', 'run', 'test', 'get', 'mod', 'install', 'fmt', 'vet'] },

  // Misc
  { text: 'tar', description: 'Archive utility', category: 'misc', subcommands: ['-xvf', '-cvf', '-tvf', '-xzf', '-czf', '-xjf', '-cjf'] },
  { text: 'gzip', description: 'Compress files', category: 'misc', subcommands: ['-d', '-k', '-v'] },
  { text: 'gunzip', description: 'Decompress files', category: 'misc' },
  { text: 'zip', description: 'Package and compress files', category: 'misc', subcommands: ['-r', '-e'] },
  { text: 'unzip', description: 'Extract ZIP archives', category: 'misc', subcommands: ['-l', '-d'] },
  { text: 'bzip2', description: 'Compress files with bzip2', category: 'misc' },
  { text: 'xz', description: 'Compress files with xz', category: 'misc' },
  { text: 'vim', description: 'Vi improved text editor', category: 'misc' },
  { text: 'vi', description: 'Vi text editor', category: 'misc' },
  { text: 'nano', description: 'Simple text editor', category: 'misc' },
  { text: 'emacs', description: 'Emacs text editor', category: 'misc' },
  { text: 'code', description: 'Visual Studio Code', category: 'misc' },
  { text: 'nvim', description: 'Neovim text editor', category: 'misc' },
  { text: 'screen', description: 'Terminal multiplexer', category: 'misc' },
  { text: 'tmux', description: 'Terminal multiplexer', category: 'misc', subcommands: ['new', 'attach', 'list-sessions', 'kill-session', 'detach'] },
  { text: 'watch', description: 'Execute command periodically', category: 'misc', subcommands: ['-n', '-d'] },
  { text: 'time', description: 'Time a command', category: 'misc' },
  { text: 'sleep', description: 'Delay for specified time', category: 'misc' },
  { text: 'yes', description: 'Output string repeatedly', category: 'misc' },
  { text: 'true', description: 'Return true', category: 'misc' },
  { text: 'false', description: 'Return false', category: 'misc' },
  { text: 'test', description: 'Evaluate conditional expression', category: 'misc' },
  { text: 'seq', description: 'Print sequence of numbers', category: 'misc' },
  { text: 'jq', description: 'JSON processor', category: 'misc' },
  { text: 'yq', description: 'YAML processor', category: 'misc' },
  { text: 'base64', description: 'Encode/decode base64', category: 'misc', subcommands: ['-d'] },
  { text: 'md5sum', description: 'Compute MD5 checksum', category: 'misc' },
  { text: 'sha256sum', description: 'Compute SHA256 checksum', category: 'misc' },
  { text: 'openssl', description: 'OpenSSL toolkit', category: 'misc' },
  { text: 'gpg', description: 'GNU Privacy Guard', category: 'misc' },
];

/**
 * Get command suggestions based on input prefix.
 * @param input - The current input text
 * @param maxResults - Maximum number of results to return
 * @returns Array of matching command suggestions
 */
export function getCommandSuggestions(input: string, maxResults = 10): CommandSuggestion[] {
  const trimmedInput = input.trim().toLowerCase();

  if (!trimmedInput) {
    return [];
  }

  // Check if we're completing a subcommand (input has space)
  const parts = trimmedInput.split(/\s+/);

  if (parts.length >= 2) {
    // User is typing a subcommand or argument
    const mainCommand = parts[0];
    const subInput = parts[parts.length - 1];

    // Find the main command
    const cmd = commonCommands.find(c => c.text === mainCommand);
    if (cmd?.subcommands) {
      // Filter subcommands that match
      const matchingSubcommands = cmd.subcommands
        .filter(sub => sub.toLowerCase().startsWith(subInput))
        .map(sub => ({
          text: sub,
          description: `${mainCommand} ${sub}`,
          category: cmd.category,
        }))
        .slice(0, maxResults);

      if (matchingSubcommands.length > 0) {
        return matchingSubcommands;
      }
    }
    return [];
  }

  // Filter commands that start with the input
  const exactStartMatches = commonCommands.filter(cmd =>
    cmd.text.toLowerCase().startsWith(trimmedInput)
  );

  // Also include fuzzy matches (contains the input)
  const containsMatches = commonCommands.filter(cmd =>
    !cmd.text.toLowerCase().startsWith(trimmedInput) &&
    cmd.text.toLowerCase().includes(trimmedInput)
  );

  return [...exactStartMatches, ...containsMatches].slice(0, maxResults);
}

/**
 * Get suggestions from command history.
 * @param history - Array of previously executed commands
 * @param input - The current input text
 * @param maxResults - Maximum number of results to return
 * @returns Array of matching history entries
 */
export function getHistorySuggestions(
  history: string[],
  input: string,
  maxResults = 5
): string[] {
  const trimmedInput = input.trim().toLowerCase();

  if (!trimmedInput) {
    return [];
  }

  // Filter history entries that start with the input
  // Use a Set to deduplicate and preserve most recent order
  const seen = new Set<string>();
  const matches: string[] = [];

  // Iterate in reverse to get most recent first
  for (let i = history.length - 1; i >= 0 && matches.length < maxResults; i--) {
    const entry = history[i];
    const entryLower = entry.toLowerCase();

    if (entryLower.startsWith(trimmedInput) && !seen.has(entryLower)) {
      seen.add(entryLower);
      matches.push(entry);
    }
  }

  return matches;
}
