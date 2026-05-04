import { FormEvent, useState } from 'react';
import { Link } from 'react-router-dom';
import { api, useApiJson } from '../api';
import type { KeyInfo } from '../api';

export default function Keys() {
  const { data, loading, error } = useApiJson<KeyInfo[]>('/api/keys');
  const [flashKey, setFlashKey] = useState<string | null>(null);

  const handleSubmit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const form = e.currentTarget;
    const label = (form.elements.namedItem('label') as HTMLInputElement).value.trim();
    if (!label) return;
    try {
      const resp = await (await api('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: `label=${encodeURIComponent(label)}`,
      })).json() as { token: string };
      setFlashKey(resp.token);
      form.reset();
    } catch (err: any) {
      alert('Failed to create key: ' + err.message);
    }
  };

  if (loading) return <div className="loading"><span className="spinner" />Loading...</div>;
  if (error) return <p className="error">Error: {error}</p>;

  return (
    <>
      {flashKey && (
        <div className="flash">
          <p>New key created. This is shown once:</p>
          <pre>{flashKey}</pre>
          <button onClick={() => setFlashKey(null)}>Dismiss</button>
        </div>
      )}
      <section>
        <form onSubmit={handleSubmit}>
          <label>Label <input name="label" type="text" required /></label>
          <button type="submit">Create key</button>
        </form>
      </section>
      <section>
        <h2>Keys</h2>
        {!data?.length ? <p className="empty">No keys yet.</p> : (
          <table>
            <thead><tr><th>Label</th><th>Created</th></tr></thead>
            <tbody>
              {data!.map((k) => (
                <tr key={k.label}>
                  <td>{k.label}</td>
                  <td>{k.created_at}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </>
  );
}
