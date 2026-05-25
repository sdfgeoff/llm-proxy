import { useEffect, useRef } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useApiJson } from '../api';
import type { DashboardMetrics } from '../api';

declare const Chart: any;

const fmt = (n: number): string => {
  if (Math.abs(n) >= 1_000_000) return (n / 1_000_000).toFixed(1).replace(/\.0$/, '') + 'M';
  if (Math.abs(n) >= 1_000) return (n / 1_000).toFixed(1).replace(/\.0$/, '') + 'K';
  if (Number.isInteger(n)) return n.toLocaleString();
  return n.toFixed(1);
};

/** Parse a UTC bucket string and format it in the browser's local time. */
const toLocal = (bucket: string): string => {
  const d = new Date(bucket + (bucket.includes(':') ? '00Z' : 'T00:00Z'));
  if (bucket.includes(':')) {
    return d.toLocaleString();
  }
  return d.toLocaleDateString();
};

const fmtMs = (ms: number): string => {
  if (ms >= 60_000) return (ms / 60_000).toFixed(1).replace(/\.0$/, '') + ' min';
  if (ms >= 1_000) return (ms / 1_000).toFixed(1).replace(/\.0$/, '') + ' s';
  if (Number.isInteger(ms)) return ms + ' ms';
  return ms.toFixed(1) + ' ms';
};

export default function Dashboard() {
  const [search, setSearch] = useSearchParams();
  const period = search.get('period') || '24h';
  const { data, loading, error } = useApiJson<DashboardMetrics>(`/api/charts?period=${period}`);
  const chartRefs = useRef<Map<string, any>>(new Map());

  useEffect(() => {
    chartRefs.current.forEach((c) => c.destroy());
    chartRefs.current.clear();
  }, [data]);

  useEffect(() => {
    if (!data) return;
    const m = data;
    const blue = '#2563eb';
    const teal = '#0f766e';
    const grid = 'rgba(0,0,0,0.08)';
    const labels = m.hourly.map((h) => toLocal(h.bucket));

    const is24h = period === '24h';
    const xTicks = is24h
      ? { maxRotation: 45, maxTicksLimit: 16 }
      : { maxRotation: 45 };

    const mk = (id: string, type: string, label: string, dsLabel: string, dsData: number[], title: string, opts?: Record<string, unknown>) => {
      const el = document.getElementById(id);
      if (!el) return;
      const c = new Chart(el, {
        type,
        data: { labels, datasets: [{ label: dsLabel, data: dsData, borderColor: label, backgroundColor: label + '22', fill: type === 'line', tension: 0.2 }] },
        options: {
          responsive: true,
          plugins: { title: { display: true, text: title, font: { size: 14 } } },
          scales: {
            x: { grid: { display: false }, ticks: xTicks },
            y: { grid: { color: grid }, beginAtZero: true },
          },
          ...opts,
        },
      });
      chartRefs.current.set(id, c);
    };

    mk('chart-requests', 'line', blue, 'Requests', m.hourly.map((h) => h.request_count), 'Requests over time');

    const stackedId = 'chart-tokens';
    const stackedEl = document.getElementById(stackedId);
    if (stackedEl) {
      const purple = '#7c3aed';
      const inputLabel = 'Input tokens';
      const outputLabel = 'Output tokens';
      const c = new Chart(stackedEl, {
        type: 'line',
        data: {
          labels,
          datasets: [
            { label: inputLabel, data: m.hourly.map((h) => h.input_tokens), borderColor: purple, backgroundColor: purple + '44', fill: true, tension: 0.2 },
            { label: outputLabel, data: m.hourly.map((h) => h.output_tokens), borderColor: blue, backgroundColor: blue + '44', fill: true, tension: 0.2 },
          ],
        },
        options: {
          responsive: true,
          plugins: { title: { display: true, text: 'Tokens over time', font: { size: 14 } }, legend: {} },
          scales: {
            x: { stacked: true, grid: { display: false }, ticks: xTicks },
            y: { stacked: true, grid: { color: grid }, beginAtZero: true },
          },
        },
      });
      chartRefs.current.set(stackedId, c);
    }
    mk('chart-tokens-per-second', 'line', teal, 'Tokens/sec', m.hourly.map((h) => h.avg_tokens_per_second || 0), 'Avg tokens/sec');
    mk('chart-ttft', 'line', blue, 'TTFT (ms)', m.hourly.map((h) => h.avg_time_to_first_token_ms || 0), 'Avg time to first token');

    const barId = 'chart-models';
    const modelsEl = document.getElementById(barId);
    if (modelsEl) {
      chartRefs.current.set(barId, new Chart(modelsEl, {
        type: 'bar',
        data: { labels: m.by_model.map((r) => r.label), datasets: [{ label: 'Token volume', data: m.by_model.map((r) => r.total_tokens), backgroundColor: blue }] },
        options: { indexAxis: 'y', responsive: true, plugins: { title: { display: true, text: 'Token volume by model', font: { size: 14 } } }, scales: { x: { grid: { color: grid }, beginAtZero: true }, y: { grid: { display: false } } } },
      }));
    }

    const keysId = 'chart-keys';
    const keysEl = document.getElementById(keysId);
    if (keysEl) {
      chartRefs.current.set(keysId, new Chart(keysEl, {
        type: 'bar',
        data: { labels: m.by_key.map((r) => r.label), datasets: [{ label: 'Token volume', data: m.by_key.map((r) => r.total_tokens), backgroundColor: teal }] },
        options: { indexAxis: 'y', responsive: true, plugins: { title: { display: true, text: 'Token volume by API key', font: { size: 14 } } }, scales: { x: { grid: { color: grid }, beginAtZero: true }, y: { grid: { display: false } } } },
      }));
    }

    const statusId = 'chart-status';
    const statusEl = document.getElementById(statusId);
    if (statusEl) {
      chartRefs.current.set(statusId, new Chart(statusEl, {
        type: 'bar',
        data: { labels: m.by_status.map((r) => r.label), datasets: [{ label: 'Requests', data: m.by_status.map((r) => r.request_count), backgroundColor: blue }] },
        options: { responsive: true, plugins: { title: { display: true, text: 'Request count by status', font: { size: 14 } } }, scales: { x: { grid: { display: false } }, y: { grid: { color: grid }, beginAtZero: true } } },
      }));
    }
  }, [data]);

  if (loading) return <div className="loading"><span className="spinner" />Loading...</div>;
  if (error) return <p className="error">Error: {error}</p>;
  if (!data) return null;
  const o = data.overview;

  return (
    <>
      <section>
        <h2>Range</h2>
        <p>
          {['24h', '7d', '30d'].map((p) => {
            const label = p === '24h' ? '24 hours' : p === '7d' ? '7 days' : '30 days';
            return p === period ? <strong key={p}>{label} </strong> : <button key={p} onClick={() => setSearch({ period: p })}>{label}</button>;
          })}
        </p>
      </section>
      <section>
        <h2>Overview</h2>
        <div className="stat-bar">
          <div className="stat-item"><span className="stat-label">Requests</span><span className="stat-value">{fmt(o.request_count)}</span></div>
          <div className="stat-item"><span className="stat-label">Total tokens</span><span className="stat-value">{fmt(o.total_tokens)}</span></div>
          <div className="stat-item"><span className="stat-label">Input tokens</span><span className="stat-value">{fmt(o.input_tokens)}</span></div>
          <div className="stat-item"><span className="stat-label">Output tokens</span><span className="stat-value">{fmt(o.output_tokens)}</span></div>
          <div className="stat-item"><span className="stat-label">Avg duration</span><span className="stat-value">{o.avg_duration_ms != null ? fmtMs(o.avg_duration_ms) : '-'}</span></div>
          <div className="stat-item"><span className="stat-label">Avg tokens/sec</span><span className="stat-value">{o.avg_tokens_per_second != null ? fmt(o.avg_tokens_per_second) : '-'}</span></div>
          <div className="stat-item"><span className="stat-label">Avg TTFT</span><span className="stat-value">{o.avg_time_to_first_token_ms != null ? fmtMs(o.avg_time_to_first_token_ms) : '-'}</span></div>
          <div className="stat-item"><span className="stat-label">Errors</span><span className="stat-value">{fmt(o.error_count)}</span></div>
        </div>
      </section>
      <section>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(480px, 1fr))', gap: 16 }}>
          <div><canvas id="chart-requests" /></div>
          <div><canvas id="chart-tokens" /></div>
          <div><canvas id="chart-tokens-per-second" /></div>
          <div><canvas id="chart-ttft" /></div>
        </div>
      </section>
      <section>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(480px, 1fr))', gap: 16 }}>
          <div><canvas id="chart-models" /></div>
          <div><canvas id="chart-keys" /></div>
          <div><canvas id="chart-status" /></div>
        </div>
      </section>
    </>
  );
}
