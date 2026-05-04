import { useEffect, useRef } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useApiJson } from '../api';
import type { DashboardMetrics } from '../api';

declare const Chart: any;

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
    const labels = m.hourly.map((h) => h.bucket);

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
            x: { grid: { display: false }, ticks: { maxRotation: 45 } },
            y: { grid: { color: grid }, beginAtZero: true },
          },
          ...opts,
        },
      });
      chartRefs.current.set(id, c);
    };

    mk('chart-requests', 'line', blue, 'Requests', m.hourly.map((h) => h.request_count), 'Requests over time');
    mk('chart-tokens', 'line', blue, 'Tokens', m.hourly.map((h) => h.total_tokens), 'Tokens over time');
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
        <dl>
          <dt>Requests</dt><dd>{o.request_count}</dd>
          <dt>Total tokens</dt><dd>{o.total_tokens}</dd>
          <dt>Input tokens</dt><dd>{o.input_tokens}</dd>
          <dt>Output tokens</dt><dd>{o.output_tokens}</dd>
          <dt>Avg duration</dt><dd>{o.avg_duration_ms != null ? `${o.avg_duration_ms} ms` : '-'}</dd>
          <dt>Avg tokens/sec</dt><dd>{o.avg_tokens_per_second ?? '-'}</dd>
          <dt>Avg TTFT</dt><dd>{o.avg_time_to_first_token_ms != null ? `${o.avg_time_to_first_token_ms} ms` : '-'}</dd>
          <dt>Errors</dt><dd>{o.error_count}</dd>
        </dl>
      </section>
      <section>
        <h2>Traffic over time</h2>
        <div style={{ maxWidth: 720 }}><canvas id="chart-requests" /></div>
        <div style={{ maxWidth: 720 }}><canvas id="chart-tokens" /></div>
        <div style={{ maxWidth: 720 }}><canvas id="chart-tokens-per-second" /></div>
        <div style={{ maxWidth: 720 }}><canvas id="chart-ttft" /></div>
      </section>
      <section>
        <h2>By model</h2>
        <div style={{ maxWidth: 720 }}><canvas id="chart-models" /></div>
      </section>
      <section>
        <h2>By API key</h2>
        <div style={{ maxWidth: 720 }}><canvas id="chart-keys" /></div>
      </section>
      <section>
        <h2>Status rates</h2>
        <div style={{ maxWidth: 720 }}><canvas id="chart-status" /></div>
      </section>
    </>
  );
}
