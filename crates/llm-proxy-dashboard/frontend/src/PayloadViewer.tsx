import { useState, useEffect } from 'react';

const MAX_INLINE_BYTES = 1024 * 1024; // 1 MB

export default function PayloadViewer({ kind, requestId }: { kind: 'request' | 'response'; requestId: string }) {
  const [json, setJson] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setJson(null);

    fetch(`/requests/${requestId}/payload/${kind}`, { credentials: 'same-origin' })
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const ct = res.headers.get('content-type') || '';
        if (!ct.includes('json')) throw new Error('Unexpected content type');
        return res.text();
      })
      .then((text) => {
        if (cancelled) return;
        if (text.length > MAX_INLINE_BYTES) {
          setError(`Payload is too large to display inline (${(text.length / 1024 / 1024).toFixed(1)} MB)`);
          return;
        }
        // Pretty-print the JSON
        setJson(JSON.stringify(JSON.parse(text), null, 2));
      })
      .catch((e: any) => {
        if (!cancelled) setError(e.message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => { cancelled = true; };
  }, [kind, requestId]);

  if (loading) return <div className="loading"><span className="spinner" />Loading payload...</div>;
  if (error) return <p className="error">{error}</p>;
  if (json == null) return <p className="empty">Payload unavailable</p>;

  return <pre>{json}</pre>;
}
