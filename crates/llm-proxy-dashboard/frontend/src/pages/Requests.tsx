import { Link } from 'react-router-dom';
import { useApiJson } from '../api';
import type { RecentRequest } from '../api';

export default function Requests() {
  const { data, loading, error } = useApiJson<RecentRequest[]>('/api/requests');

  if (loading) return <div className="loading"><span className="spinner" />Loading...</div>;
  if (error) return <p className="error">Error: {error}</p>;

  return (
    <section>
      <h2>Recent requests</h2>
      {!data?.length ? <p className="empty">No requests yet.</p> : (
        <table>
          <thead>
            <tr><th>Started</th><th>Key</th><th>Endpoint</th><th>Model</th><th>Route</th><th>Status</th><th>Duration</th><th>Tokens</th><th>Payload</th></tr>
          </thead>
          <tbody>
            {data!.map((r) => (
              <tr key={r.id}>
                <td><Link to={`/requests/${r.id}`}>{r.started_at}</Link></td>
                <td>{r.proxy_key_label ?? '-'}</td>
                <td>{r.endpoint}</td>
                <td>{r.requested_model ?? '-'}</td>
                <td>{r.route_name ?? '-'}</td>
                <td>{r.http_status ?? '-'}</td>
                <td>{r.duration_ms != null ? `${r.duration_ms} ms` : '-'}</td>
                <td>{r.total_tokens ?? '-'}</td>
                <td>{r.payload_capture_status}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
