import { useParams, Link } from 'react-router-dom';
import { useApiJson } from '../api';
import type { RequestDetail } from '../api';
import PayloadViewer from '../PayloadViewer';

export default function RequestDetail() {
  const { id } = useParams<{ id: string }>();
  const { data, loading, error } = useApiJson<RequestDetail>(`/api/requests/${id}`);

  if (loading) return <div className="loading"><span className="spinner" />Loading...</div>;
  if (error) return <p className="error">Error: {error}</p>;
  if (!data) return null;
  const r = data;

  return (
    <>
      <section>
        <Link to="/requests">← Back</Link>
        <h2>Request {r.id}</h2>
        <dl>
          <dt>Started</dt><dd>{r.started_at}</dd>
          <dt>API key</dt><dd>{r.proxy_key_label ?? '-'}</dd>
          <dt>Endpoint</dt><dd>{r.endpoint}</dd>
          <dt>Requested model</dt><dd>{r.requested_model ?? '-'}</dd>
          <dt>Upstream model</dt><dd>{r.upstream_model ?? '-'}</dd>
          <dt>Route</dt><dd>{r.route_name ?? '-'}</dd>
          <dt>Routing match</dt><dd>{r.routing_match ?? '-'}</dd>
          <dt>Stream</dt><dd>{r.stream ? 'yes' : 'no'}</dd>
          <dt>Status</dt><dd>{r.http_status ?? '-'}</dd>
          <dt>Error category</dt><dd>{r.error_category ?? '-'}</dd>
          <dt>Duration</dt><dd>{r.duration_ms != null ? `${r.duration_ms} ms` : '-'}</dd>
          <dt>Upstream first byte</dt><dd>{r.upstream_first_byte_ms != null ? `${r.upstream_first_byte_ms} ms` : '-'}</dd>
          <dt>Time to first token</dt><dd>{r.time_to_first_token_ms != null ? `${r.time_to_first_token_ms} ms` : '-'}</dd>
          <dt>Generation duration</dt><dd>{r.generation_ms != null ? `${r.generation_ms} ms` : '-'}</dd>
          <dt>Token source</dt><dd>{r.token_source ?? '-'}</dd>
          <dt>Input tokens</dt><dd>{r.input_tokens ?? '-'}</dd>
          <dt>Output tokens</dt><dd>{r.output_tokens ?? '-'}</dd>
          <dt>Total tokens</dt><dd>{r.total_tokens ?? '-'}</dd>
          <dt>Cached input tokens</dt><dd>{r.cached_input_tokens ?? '-'}</dd>
          <dt>Reasoning tokens</dt><dd>{r.reasoning_tokens ?? '-'}</dd>
          <dt>Payload capture</dt><dd>{r.payload_capture_status}</dd>
        </dl>
      </section>
      <section>
        <h2>Request payload</h2>
        <PayloadViewer kind="request" requestId={r.id} />
      </section>
      <section>
        <h2>Response payload</h2>
        <PayloadViewer kind="response" requestId={r.id} />
      </section>
      {r.payload_capture_error && <section><p className="error">Capture error: {r.payload_capture_error}</p></section>}
      {r.provider_usage_json && <section><h2>Provider usage</h2><pre>{r.provider_usage_json}</pre></section>}
    </>
  );
}
