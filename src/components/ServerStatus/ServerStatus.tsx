import { useState, useEffect, useCallback, useRef, memo } from 'react';
import {
  Cpu,
  MemoryStick,
  HardDrive,
  Network,
  RefreshCw,
  Clock,
  Server,
  ChevronDown,
  ChevronUp,
  Activity,
  AlertCircle,
  Loader2,
  GripHorizontal,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import { safeInvoke } from '../../lib/tauri';

// =============================================================================
// Types matching the Rust backend
// =============================================================================

interface CpuInfo {
  usagePercent: number;
  coreCount: number;
  loadAverage: [number, number, number];
}

interface MemoryInfo {
  total: number;
  used: number;
  free: number;
  available: number;
  usagePercent: number;
  swapTotal: number;
  swapUsed: number;
}

interface DiskInfo {
  mountPoint: string;
  filesystem: string;
  total: number;
  used: number;
  available: number;
  usagePercent: number;
}

interface NetworkInfo {
  interface: string;
  rxBytes: number;
  txBytes: number;
  rxPackets: number;
  txPackets: number;
}

interface ServerStatus {
  hostname: string;
  uptimeSeconds: number;
  cpu: CpuInfo;
  memory: MemoryInfo;
  disks: DiskInfo[];
  network: NetworkInfo[];
  collectedAt: number;
}

// =============================================================================
// Refresh Interval Options
// =============================================================================

export type RefreshInterval = '5s' | '10s' | '30s' | '1m' | '5m' | 'manual';

export const refreshIntervalOptions: { value: RefreshInterval; label: string; ms: number | null }[] = [
  { value: '5s', label: '5 seconds', ms: 5000 },
  { value: '10s', label: '10 seconds', ms: 10000 },
  { value: '30s', label: '30 seconds', ms: 30000 },
  { value: '1m', label: '1 minute', ms: 60000 },
  { value: '5m', label: '5 minutes', ms: 300000 },
  { value: 'manual', label: 'Manual', ms: null },
];

// =============================================================================
// Helper Functions
// =============================================================================

/**
 * Format bytes to human readable format
 */
function formatBytes(bytes: number, decimals = 1): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
}

/**
 * Format uptime to human readable format
 */
function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);

  const parts: string[] = [];
  if (days > 0) parts.push(`${days}d`);
  if (hours > 0) parts.push(`${hours}h`);
  if (minutes > 0) parts.push(`${minutes}m`);

  return parts.length > 0 ? parts.join(' ') : '< 1m';
}

/**
 * Get color based on usage percentage
 */
function getUsageColor(percent: number): string {
  if (percent >= 90) return 'text-tokyo-red';
  if (percent >= 70) return 'text-tokyo-orange';
  if (percent >= 50) return 'text-tokyo-yellow';
  return 'text-tokyo-green';
}

/**
 * Get background color class for progress bar
 */
function getProgressColor(percent: number): string {
  if (percent >= 90) return 'bg-tokyo-red';
  if (percent >= 70) return 'bg-tokyo-orange';
  if (percent >= 50) return 'bg-tokyo-yellow';
  return 'bg-tokyo-green';
}

// =============================================================================
// Sub-Components
// =============================================================================

interface ProgressBarProps {
  value: number;
  max?: number;
  className?: string;
  showPercent?: boolean;
}

// Memoized progress bar component for performance
const ProgressBar = memo(function ProgressBar({ value, max = 100, className, showPercent = true }: ProgressBarProps) {
  const percent = Math.min((value / max) * 100, 100);

  return (
    <div className={cn('flex items-center gap-2', className)}>
      <div className="flex-1 h-2 bg-tokyo-bg-hl rounded-full overflow-hidden">
        <div
          className={cn('h-full transition-all duration-300', getProgressColor(percent))}
          style={{ width: `${percent}%` }}
        />
      </div>
      {showPercent && (
        <span className={cn('text-xs font-mono w-12 text-right', getUsageColor(percent))}>
          {percent.toFixed(1)}%
        </span>
      )}
    </div>
  );
});

interface MetricCardProps {
  icon: React.ReactNode;
  title: string;
  children: React.ReactNode;
  className?: string;
}

// Memoized metric card component for performance
const MetricCard = memo(function MetricCard({ icon, title, children, className }: MetricCardProps) {
  return (
    <div className={cn('bg-tokyo-bg rounded-lg p-3 border border-tokyo-bg-hl', className)}>
      <div className="flex items-center gap-2 mb-2">
        <span className="text-tokyo-blue">{icon}</span>
        <h3 className="text-sm font-medium text-tokyo-fg">{title}</h3>
      </div>
      {children}
    </div>
  );
});

// =============================================================================
// Main Component
// =============================================================================

interface ServerStatusProps {
  /** Session ID to monitor */
  sessionId: string;
  /** Whether the panel is initially collapsed */
  defaultCollapsed?: boolean;
  /** Default refresh interval */
  defaultRefreshInterval?: RefreshInterval;
  /** Default height of the panel in pixels */
  defaultHeight?: number;
  /** Minimum height of the panel in pixels */
  minHeight?: number;
  /** Maximum height of the panel in pixels */
  maxHeight?: number;
  /** Callback when panel is collapsed/expanded */
  onToggle?: (collapsed: boolean) => void;
}

export function ServerStatus({
  sessionId,
  defaultCollapsed = true,
  defaultRefreshInterval = '30s',
  defaultHeight = 220,
  minHeight = 120,
  maxHeight = 500,
  onToggle,
}: ServerStatusProps) {
  const [isCollapsed, setIsCollapsed] = useState(defaultCollapsed);
  const [panelHeight, setPanelHeight] = useState(defaultHeight);
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshInterval, setRefreshInterval] = useState<RefreshInterval>(defaultRefreshInterval);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);

  // Resize drag state
  const [isDragging, setIsDragging] = useState(false);
  const dragStartY = useRef<number>(0);
  const dragStartHeight = useRef<number>(0);
  const panelRef = useRef<HTMLDivElement>(null);

  // Reference to the previous network stats for calculating rates
  const prevNetworkRef = useRef<Map<string, { rx: number; tx: number; time: number }>>(new Map());
  const [networkRates, setNetworkRates] = useState<Map<string, { rxRate: number; txRate: number }>>(new Map());

  // Handle resize dragging
  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      const deltaY = dragStartY.current - e.clientY;
      const newHeight = Math.min(maxHeight, Math.max(minHeight, dragStartHeight.current + deltaY));
      setPanelHeight(newHeight);
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    document.body.style.cursor = 'ns-resize';
    document.body.style.userSelect = 'none';

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging, minHeight, maxHeight]);

  // Start resize drag
  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragStartY.current = e.clientY;
    dragStartHeight.current = panelHeight;
    setIsDragging(true);
  }, [panelHeight]);

  const fetchStatus = useCallback(async () => {
    if (!sessionId) return;

    setIsLoading(true);
    setError(null);

    try {
      const result = await safeInvoke<ServerStatus>('get_server_status', {
        request: { sessionId },
      });

      if (result.success) {
        setStatus(result.data);
        setLastUpdated(new Date());

        // Calculate network rates
        const now = Date.now();
        const newRates = new Map<string, { rxRate: number; txRate: number }>();

        for (const iface of result.data.network) {
          const prev = prevNetworkRef.current.get(iface.interface);
          if (prev) {
            const timeDelta = (now - prev.time) / 1000; // seconds
            if (timeDelta > 0) {
              const rxRate = (iface.rxBytes - prev.rx) / timeDelta;
              const txRate = (iface.txBytes - prev.tx) / timeDelta;
              newRates.set(iface.interface, { rxRate: Math.max(0, rxRate), txRate: Math.max(0, txRate) });
            }
          }
          prevNetworkRef.current.set(iface.interface, {
            rx: iface.rxBytes,
            tx: iface.txBytes,
            time: now,
          });
        }

        setNetworkRates(newRates);
      } else {
        throw new Error(result.error.message);
      }
    } catch (err) {
      console.error('[ServerStatus] Failed to fetch status:', err);
      setError(err instanceof Error ? err.message : 'Failed to fetch server status');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId]);

  // Auto-refresh based on interval
  useEffect(() => {
    // Don't fetch if collapsed or manual refresh selected
    if (isCollapsed || refreshInterval === 'manual') {
      return;
    }

    // Initial fetch when expanded
    fetchStatus();

    // Set up interval
    const intervalMs = refreshIntervalOptions.find((opt) => opt.value === refreshInterval)?.ms;
    if (intervalMs) {
      const timer = setInterval(fetchStatus, intervalMs);
      return () => clearInterval(timer);
    }
  }, [isCollapsed, refreshInterval, fetchStatus]);

  // Fetch when expanding panel for the first time
  useEffect(() => {
    if (!isCollapsed && !status && !isLoading) {
      fetchStatus();
    }
  }, [isCollapsed, status, isLoading, fetchStatus]);

  const handleToggle = useCallback(() => {
    const newCollapsed = !isCollapsed;
    setIsCollapsed(newCollapsed);
    onToggle?.(newCollapsed);
  }, [isCollapsed, onToggle]);

  const handleRefreshIntervalChange = useCallback((interval: RefreshInterval) => {
    setRefreshInterval(interval);
    // Clear previous network stats when changing interval
    prevNetworkRef.current.clear();
    setNetworkRates(new Map());
  }, []);

  if (!sessionId) {
    return null;
  }

  // Calculate panel height based on state
  const getPanelHeight = () => {
    if (isCollapsed) return '40px';
    return `${panelHeight}px`;
  };

  return (
    <div
      ref={panelRef}
      className="relative border-t border-tokyo-bg-hl bg-tokyo-bg-dark transition-all duration-200"
      style={{ height: getPanelHeight() }}
    >
      {/* Resize Handle (when not collapsed) */}
      {!isCollapsed && (
        <div
          className={cn(
            'absolute top-0 left-0 right-0 h-2 cursor-ns-resize z-10',
            'group flex items-center justify-center',
            'hover:bg-tokyo-blue/30 transition-colors',
            isDragging && 'bg-tokyo-blue/50'
          )}
          onMouseDown={handleResizeStart}
          title="Drag to resize"
        >
          <div className={cn(
            'flex items-center justify-center w-12 h-4 -mt-2 rounded-t',
            'bg-tokyo-bg-hl/80 border border-b-0 border-tokyo-bg-hl',
            'opacity-60 group-hover:opacity-100 transition-opacity',
            isDragging && 'opacity-100 bg-tokyo-blue/30'
          )}>
            <GripHorizontal className="w-4 h-4 text-tokyo-comment" />
          </div>
        </div>
      )}

      {/* Header */}
      <div
        className={cn(
          'flex items-center justify-between px-3 h-10',
          'border-b border-tokyo-bg-hl cursor-pointer',
          'hover:bg-tokyo-bg-hl/30 transition-colors duration-150'
        )}
        onClick={handleToggle}
      >
        <div className="flex items-center gap-2">
          <button
            className="p-1 hover:bg-tokyo-bg-hl rounded transition-colors"
            onClick={(e) => {
              e.stopPropagation();
              handleToggle();
            }}
            aria-label={isCollapsed ? 'Expand server status' : 'Collapse server status'}
          >
            {isCollapsed ? (
              <ChevronUp className="w-4 h-4 text-tokyo-comment" />
            ) : (
              <ChevronDown className="w-4 h-4 text-tokyo-comment" />
            )}
          </button>
          <Activity className="w-4 h-4 text-tokyo-cyan" />
          <span className="text-sm font-medium text-tokyo-fg">Server Status</span>
          {isLoading && <Loader2 className="w-4 h-4 text-tokyo-blue animate-spin ml-2" />}
          {status && !isCollapsed && (
            <span className="text-xs text-tokyo-comment ml-2">
              {status.hostname}
            </span>
          )}
        </div>

        <div className="flex items-center gap-2" onClick={(e) => e.stopPropagation()}>
          {/* Refresh Interval Selector */}
          <select
            value={refreshInterval}
            onChange={(e) => handleRefreshIntervalChange(e.target.value as RefreshInterval)}
            className="px-2 py-1 text-xs rounded bg-tokyo-bg-hl border border-tokyo-bg-hl
                       text-tokyo-fg cursor-pointer
                       focus:outline-none focus:ring-1 focus:ring-tokyo-blue"
          >
            {refreshIntervalOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>

          {/* Manual Refresh Button */}
          <button
            className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
            onClick={(e) => {
              e.stopPropagation();
              fetchStatus();
            }}
            disabled={isLoading}
            title="Refresh now"
          >
            <RefreshCw className={cn('w-4 h-4 text-tokyo-comment', isLoading && 'animate-spin')} />
          </button>
        </div>
      </div>

      {/* Content */}
      {!isCollapsed && (
        <div className="h-[calc(100%-40px)] overflow-auto p-3 space-y-3">
          {/* Error Display */}
          {error && (
            <div className="flex items-center gap-2 p-2 rounded bg-tokyo-red/10 border border-tokyo-red/30 text-tokyo-red text-sm">
              <AlertCircle className="w-4 h-4 flex-shrink-0" />
              <span className="truncate">{error}</span>
              <button
                onClick={() => setError(null)}
                className="ml-auto text-tokyo-red hover:text-tokyo-red/80 text-xs"
              >
                Dismiss
              </button>
            </div>
          )}

          {/* Loading State */}
          {isLoading && !status && (
            <div className="flex items-center justify-center py-8 text-tokyo-comment">
              <Loader2 className="w-6 h-6 animate-spin mr-2" />
              <span>Loading server status...</span>
            </div>
          )}

          {/* Status Display */}
          {status && (
            <>
              {/* Top Row: Server Info + Uptime */}
              <div className="flex items-center justify-between text-xs text-tokyo-comment">
                <div className="flex items-center gap-2">
                  <Server className="w-4 h-4" />
                  <span className="font-medium text-tokyo-fg">{status.hostname}</span>
                </div>
                <div className="flex items-center gap-4">
                  <div className="flex items-center gap-1">
                    <Clock className="w-3 h-3" />
                    <span>Uptime: {formatUptime(status.uptimeSeconds)}</span>
                  </div>
                  {lastUpdated && (
                    <span>Updated: {lastUpdated.toLocaleTimeString()}</span>
                  )}
                </div>
              </div>

              {/* Metrics Grid */}
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3">
                {/* CPU */}
                <MetricCard icon={<Cpu className="w-4 h-4" />} title="CPU">
                  <ProgressBar value={status.cpu.usagePercent} />
                  <div className="mt-2 space-y-1 text-xs text-tokyo-comment">
                    <div className="flex justify-between">
                      <span>Cores</span>
                      <span className="text-tokyo-fg">{status.cpu.coreCount}</span>
                    </div>
                    <div className="flex justify-between">
                      <span>Load Avg</span>
                      <span className="text-tokyo-fg font-mono">
                        {status.cpu.loadAverage.map((l) => l.toFixed(2)).join(' ')}
                      </span>
                    </div>
                  </div>
                </MetricCard>

                {/* Memory */}
                <MetricCard icon={<MemoryStick className="w-4 h-4" />} title="Memory">
                  <ProgressBar value={status.memory.usagePercent} />
                  <div className="mt-2 space-y-1 text-xs text-tokyo-comment">
                    <div className="flex justify-between">
                      <span>Used / Total</span>
                      <span className="text-tokyo-fg">
                        {formatBytes(status.memory.used)} / {formatBytes(status.memory.total)}
                      </span>
                    </div>
                    {status.memory.swapTotal > 0 && (
                      <div className="flex justify-between">
                        <span>Swap</span>
                        <span className="text-tokyo-fg">
                          {formatBytes(status.memory.swapUsed)} / {formatBytes(status.memory.swapTotal)}
                        </span>
                      </div>
                    )}
                  </div>
                </MetricCard>

                {/* Disk */}
                <MetricCard icon={<HardDrive className="w-4 h-4" />} title="Disk">
                  {status.disks.length > 0 ? (
                    <div className="space-y-2">
                      {status.disks.slice(0, 2).map((disk) => (
                        <div key={disk.mountPoint}>
                          <div className="flex justify-between text-xs mb-1">
                            <span className="text-tokyo-comment truncate max-w-[80px]" title={disk.mountPoint}>
                              {disk.mountPoint}
                            </span>
                            <span className="text-tokyo-fg">
                              {formatBytes(disk.used)} / {formatBytes(disk.total)}
                            </span>
                          </div>
                          <ProgressBar value={disk.usagePercent} showPercent={false} />
                        </div>
                      ))}
                    </div>
                  ) : (
                    <span className="text-xs text-tokyo-comment">No disk info available</span>
                  )}
                </MetricCard>

                {/* Network */}
                <MetricCard icon={<Network className="w-4 h-4" />} title="Network">
                  {status.network.length > 0 ? (
                    <div className="space-y-2 text-xs">
                      {status.network.slice(0, 2).map((iface) => {
                        const rates = networkRates.get(iface.interface);
                        return (
                          <div key={iface.interface}>
                            <div className="text-tokyo-comment mb-1">{iface.interface}</div>
                            <div className="grid grid-cols-2 gap-2">
                              <div>
                                <span className="text-tokyo-comment">RX: </span>
                                <span className="text-tokyo-green">
                                  {rates ? `${formatBytes(rates.rxRate)}/s` : formatBytes(iface.rxBytes)}
                                </span>
                              </div>
                              <div>
                                <span className="text-tokyo-comment">TX: </span>
                                <span className="text-tokyo-cyan">
                                  {rates ? `${formatBytes(rates.txRate)}/s` : formatBytes(iface.txBytes)}
                                </span>
                              </div>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  ) : (
                    <span className="text-xs text-tokyo-comment">No network info available</span>
                  )}
                </MetricCard>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

export type { ServerStatusProps, ServerStatus as ServerStatusData };
