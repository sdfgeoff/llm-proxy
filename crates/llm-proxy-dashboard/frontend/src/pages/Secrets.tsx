import { FormEvent } from 'react';
import { useSearchParams } from 'react-router-dom';
import { api, useApiJson } from '../api';
import type { SecretInfo } from '../api';

export default function Secrets() {
  const [search] = useSearchParams();
  const flash = search.get('flash');
  const { data, loading, error } = useApiJson<SecretInfo[]>('/api/secrets');

  const handleSubmit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const form = e.currentTarget;
    const name = (form.elements.namedItem('name') as HTMLInputElement).value.trim();
    const value = (form.elements.namedItem('value') as HTMLInputElement).value;
    if (!name || !value) return;
    try {
      await api('/api/secrets', {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: `name=${encodeURIComponent(name)}&value=${encodeURIComponent(value)}`,
      });
      window.location.href = '/secrets?flash=updated';
    } catch (err: any) {
      alert('Failed to save secret: ' + err.message);
    }
  };

  const handleDelete = async (name: string) => {
    try {
      await api('/api/secrets/delete', {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: `name=${encodeURIComponent(name)}`,
      });
      window.location.href = '/secrets?flash=deleted';
    } catch (err: any) {
      alert('Failed to delete secret: ' + err.message);
    }
  };

  if (loading) return <div className="loading"><span className="spinner" />Loading...</div>;
  if (error) return <p className="error">Error: {error}</p>;

  return (
    <>
      {flash === 'updated' && <div className="flash">Secret saved.</div>}
      {flash === 'deleted' && <div className="flash">Secret deleted.</div>}
      <section>
        <form onSubmit={handleSubmit}>
          <label>Name <input name="name" type="text" required /></label>
          <label>API key <input name="value" type="password" required /></label>
          <button type="submit">Save secret</button>
        </form>
      </section>
      <section>
        <h2>Upstream secrets</h2>
        {!data?.length ? <p className="empty">No secrets yet.</p> : (
          <table>
            <thead><tr><th>Name</th><th>Updated</th><th></th></tr></thead>
            <tbody>
              {data!.map((s) => (
                <tr key={s.name}>
                  <td>{s.name}</td>
                  <td>{s.updated_at}</td>
                  <td><button className="danger" onClick={() => handleDelete(s.name)}>Delete</button></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </>
  );
}
