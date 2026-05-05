import { useState, useEffect } from 'react';

/* ── fetch helpers ────────────────────────────────────── */

async function fetchRaw(path: string, init?: RequestInit): Promise<Response> {
  const res = await fetch(path, { credentials: 'same-origin', ...init });
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(text || `HTTP ${res.status}`);
  }
  return res;
}

async function fetchJson<T = unknown>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetchRaw(path, init);
  return res.json() as Promise<T>;
}

/** Namespace for API helpers. Callable as `api(path)` for raw, or `api.json(path)`. */
export const api: ((path: string, init?: RequestInit) => Promise<Response>) & {
  json: typeof fetchJson;
} = fetchRaw as any;
api.json = fetchJson;

/** React hook: fetch JSON, re-fetch on mount. */
export function useApiJson<T>(path: string) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refetch = () => {
    setLoading(true);
    setError(null);
    fetchJson<T>(path)
      .then((d) => { setData(d); setLoading(false); })
      .catch((e: any) => { setError(e.message); setLoading(false); });
  };

  useEffect(() => { refetch(); }, [path]);

  return { data, loading, error, refetch };
}

/* ── types ────────────────────────────────────────────── */

export interface Overview {
  request_count: number;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  avg_duration_ms: number | null;
  avg_tokens_per_second: number | null;
  avg_time_to_first_token_ms: number | null;
  error_count: number;
}

export interface HourlyBucket {
  bucket: string;
  request_count: number;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  avg_tokens_per_second: number | null;
  avg_time_to_first_token_ms: number | null;
}

export interface BreakdownRow {
  label: string;
  total_tokens: number;
  request_count: number;
}

export interface DashboardMetrics {
  overview: Overview;
  hourly: HourlyBucket[];
  by_model: BreakdownRow[];
  by_key: BreakdownRow[];
  by_status: BreakdownRow[];
}

export interface RecentRequest {
  id: string;
  started_at: string;
  proxy_key_label: string | null;
  endpoint: string;
  requested_model: string | null;
  route_name: string | null;
  http_status: number | null;
  duration_ms: number | null;
  total_tokens: number | null;
  payload_capture_status: string;
}

export interface RequestDetail {
  id: string;
  started_at: string;
  proxy_key_label: string | null;
  endpoint: string;
  requested_model: string | null;
  upstream_model: string | null;
  route_name: string | null;
  routing_match: string | null;
  stream: boolean;
  http_status: number | null;
  error_category: string | null;
  duration_ms: number | null;
  upstream_first_byte_ms: number | null;
  time_to_first_token_ms: number | null;
  generation_ms: number | null;
  token_source: string | null;
  input_tokens: number | null;
  output_tokens: number | null;
  total_tokens: number | null;
  cached_input_tokens: number | null;
  reasoning_tokens: number | null;
  payload_capture_status: string;
  request_payload_path: string | null;
  response_payload_path: string | null;
  request_payload_bytes: number | null;
  response_payload_bytes: number | null;
  payload_capture_error: string | null;
  provider_usage_json: string | null;
}

export interface KeyInfo {
  id: string;
  label: string;
  created_at: string;
}

export interface SecretInfo {
  name: string;
  updated_at: string;
}
