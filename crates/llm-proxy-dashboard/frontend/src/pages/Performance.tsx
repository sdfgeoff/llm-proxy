import { useEffect, useRef, useState, useCallback } from 'react';
import { useApiJson } from '../api';
import type { PerformanceSnapshot } from '../api';

declare const Chart: any;

const fmtBytes = (bytes: number): string => {
  if (bytes >= 1_073_741_824) return (bytes / 1_073_741_824).toFixed(1).replace(/\.0$/, '') + ' GB/s';
  if (bytes >= 1_048_576) return (bytes / 1_048_576).toFixed(1).replace(/\.0$/, '') + ' MB/s';
  if (bytes >= 1024) return (bytes / 1024).toFixed(1).replace(/\.0$/, '') + ' KB/s';
  return bytes + ' B/s';
};

const fmtMb = (mb: number): string => {
  if (mb >= 1024) return (mb / 1024).toFixed(1).replace(/\.0$/, '') + ' GB';
  return mb + ' MB';
};

const toLocal = (iso: string): string => {
  const d = new Date(iso);
  return d.toLocaleTimeString();
};

export default function Performance() {
  const { data: history, loading: historyLoading } = useApiJson<PerformanceSnapshot[]>('/api/performance/history');
  const latestRef = useRef<PerformanceSnapshot | null>(null);
  const [latest, setLatest] = useState<PerformanceSnapshot | null>(null);
  const chartRefs = useRef<Map<string, any>>(new Map());
  const wsRef = useRef<WebSocket | null>(null);
  const maxDataPoints = 60; // ~5 minutes at 5s intervals

  // WebSocket for live updates
  const connectWs = useCallback(() => {
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }

    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${proto}//${window.location.host}/ws/performance`);
    wsRef.current = ws;

    ws.onmessage = (event) => {
      try {
        const snapshot: PerformanceSnapshot = JSON.parse(event.data);
        latestRef.current = snapshot;
        setLatest(snapshot);
      } catch {
        // ignore parse errors
      }
    };

    ws.onclose = () => {
      // Reconnect after 3 seconds
      setTimeout(connectWs, 3000);
    };
  }, []);

  useEffect(() => {
    connectWs();
    return () => {
      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }
    };
  }, [connectWs]);

  // Use latest from WebSocket if available, otherwise from history
  const displayLatest = latest || (history && history.length > 0 ? history[history.length - 1] : null);

  // Build chart data from history, updated with latest WebSocket data
  const chartData = useRef<PerformanceSnapshot[]>([]);

  useEffect(() => {
    if (history && history.length > 0) {
      chartData.current = [...history];
    }
  }, [history]);

  // Push latest WebSocket snapshot into chart data
  useEffect(() => {
    if (!latest || chartData.current.length === 0) return;
    const last = chartData.current[chartData.current.length - 1];
    if (last && latest.timestamp === last.timestamp) return; // duplicate
    chartData.current.push(latest);
    if (chartData.current.length > maxDataPoints) {
      chartData.current = chartData.current.slice(-maxDataPoints);
    }
  }, [latest]);

  // Render charts
  useEffect(() => {
    chartRefs.current.forEach((c) => c.destroy());
    chartRefs.current.clear();

    const data = chartData.current;
    if (!data || data.length < 2) return;

    const blue = '#2563eb';
    const teal = '#0f766e';
    const purple = '#7c3aed';
    const orange = '#ea580c';
    const green = '#16a34a';
    const grid = 'rgba(0,0,0,0.08)';
    const labels = data.map((s) => toLocal(s.timestamp));
    const xTicks = { maxRotation: 45, maxTicksLimit: 12 };

    const mk = (id: string, label: string, color: string, dsData: number[], title: string, suffix = '') => {
      const el = document.getElementById(id);
      if (!el) return;
      const c = new Chart(el, {
        type: 'line',
        data: { labels, datasets: [{ label, data: dsData, borderColor: color, backgroundColor: color + '22', fill: true, tension: 0.2, pointRadius: 0 }] },
        options: {
          responsive: true,
          animation: false,
          plugins: { title: { display: true, text: title, font: { size: 14 } } },
          scales: {
            x: { grid: { display: false }, ticks: xTicks },
            y: { grid: { color: grid }, beginAtZero: true, ticks: { callback: (v: number) => v + suffix } },
          },
        },
      });
      chartRefs.current.set(id, c);
    };

    mk('chart-cpu', 'CPU %', blue, data.map((s) => s.cpu.usage_percent), 'CPU usage', '%');
    mk('chart-ram', 'RAM %', teal, data.map((s) => s.ram.usage_percent), 'Memory usage', '%');
    mk('chart-load', 'Load (1m)', purple, data.map((s) => s.load_average.load1), 'System load average');

    // GPU chart (only if GPUs exist)
    if (data.some((s) => s.gpus && s.gpus.length > 0)) {
      const gpuId = 'chart-gpu';
      const gpuEl = document.getElementById(gpuId);
      if (gpuEl && data[0].gpus.length > 0) {
        const gpuNames = data[0].gpus.map((g, i) => g.name || `GPU ${i}`);
        const gpuColors = [orange, green, blue, purple];
        const datasets = data[0].gpus.map((g, i) => ({
          label: `${g.name || 'GPU ' + i} util %`,
          data: data.map((s) => (s.gpus[i] ? s.gpus[i].utilization_percent : 0)),
          borderColor: gpuColors[i % gpuColors.length],
          backgroundColor: gpuColors[i % gpuColors.length] + '22',
          fill: true as any,
          tension: 0.2,
          pointRadius: 0,
        }));
        const c = new Chart(gpuEl, {
          type: 'line',
          data: { labels, datasets },
          options: {
            responsive: true,
            animation: false,
            plugins: { title: { display: true, text: 'GPU utilization', font: { size: 14 } }, legend: { display: gpuNames.length > 1 } },
            scales: {
              x: { grid: { display: false }, ticks: xTicks },
              y: { grid: { color: grid }, beginAtZero: true, max: 100, ticks: { callback: (v: number) => v + '%' } },
            },
          },
        });
        chartRefs.current.set(gpuId, c);
      }
    }

    // Network chart
    const netId = 'chart-network';
    const netEl = document.getElementById(netId);
    if (netEl && data.some((s) => s.networks && s.networks.length > 0)) {
      // Sum all non-lo interfaces
      const rxData = data.map((s) => s.networks.filter((n) => n.interface !== 'lo').reduce((sum, n) => sum + n.bytes_received, 0));
      const txData = data.map((s) => s.networks.filter((n) => n.interface !== 'lo').reduce((sum, n) => sum + n.bytes_transmitted, 0));
      const c = new Chart(netEl, {
        type: 'line',
        data: {
          labels,
          datasets: [
            { label: 'Received', data: rxData, borderColor: blue, backgroundColor: blue + '22', fill: true, tension: 0.2, pointRadius: 0 },
            { label: 'Transmitted', data: txData, borderColor: teal, backgroundColor: teal + '22', fill: true, tension: 0.2, pointRadius: 0 },
          ],
        },
        options: {
          responsive: true,
          animation: false,
          plugins: { title: { display: true, text: 'Network throughput', font: { size: 14 } }, legend: {} },
          scales: {
            x: { grid: { display: false }, ticks: xTicks },
            y: { grid: { color: grid }, beginAtZero: true, ticks: { callback: (v: number) => fmtBytes(v) } },
          },
        },
      });
      chartRefs.current.set(netId, c);
    }
  }, [chartData.current]);

  if (historyLoading) return <div className="loading"><span className="spinner" />Loading...</div>;

  return (
    <>
      <section>
        <h2>System overview</h2>
        {displayLatest ? (
          <div className="stat-bar">
            <div className="stat-item">
              <span className="stat-label">CPU</span>
              <span className="stat-value">{displayLatest.cpu.usage_percent.toFixed(1)}%</span>
            </div>
            <div className="stat-item">
              <span className="stat-label">RAM</span>
              <span className="stat-value">{displayLatest.ram.usage_percent.toFixed(1)}% ({fmtMb(displayLatest.ram.used_mb)} / {fmtMb(displayLatest.ram.total_mb)})</span>
            </div>
            <div className="stat-item">
              <span className="stat-label">Load</span>
              <span className="stat-value">{displayLatest.load_average.load1.toFixed(2)} / {displayLatest.load_average.load5.toFixed(2)} / {displayLatest.load_average.load15.toFixed(2)}</span>
            </div>
            {displayLatest.gpus.length > 0 && displayLatest.gpus.map((gpu, i) => (
              <div className="stat-item" key={i}>
                <span className="stat-label">{gpu.name || 'GPU ' + gpu.index}</span>
                <span className="stat-value">{gpu.utilization_percent.toFixed(0)}% ({fmtMb(gpu.vram_used_mb)} / {fmtMb(gpu.vram_total_mb)}) {gpu.temperature_c > 0 ? gpu.temperature_c.toFixed(0) + '°C' : ''}</span>
              </div>
            ))}
            {displayLatest.disks.map((disk, i) => disk.mount && (
              <div className="stat-item" key={i}>
                <span className="stat-label">Disk {disk.mount}</span>
                <span className="stat-value">{disk.usage_percent.toFixed(1)}% ({fmtMb(disk.used_mb)} / {fmtMb(disk.total_mb)})</span>
              </div>
            ))}
            {displayLatest.cpu_temps.length > 0 && displayLatest.cpu_temps.map((t, i) => (
              <div className="stat-item" key={i}>
                <span className="stat-label">Temp {t.name}</span>
                <span className="stat-value">{t.temperature_c.toFixed(1)}°C</span>
              </div>
            ))}
          </div>
        ) : (
          <p className="empty">Collecting metrics...</p>
        )}
      </section>

      <section>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(480px, 1fr))', gap: 16 }}>
          <div><canvas id="chart-cpu" /></div>
          <div><canvas id="chart-ram" /></div>
          <div><canvas id="chart-load" /></div>
          {chartData.current.some((s) => s.gpus && s.gpus.length > 0) && <div><canvas id="chart-gpu" /></div>}
          <div><canvas id="chart-network" /></div>
        </div>
      </section>
    </>
  );
}
