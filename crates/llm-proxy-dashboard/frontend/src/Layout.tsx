import { Outlet, Link, useNavigate } from 'react-router-dom';
import { api } from './api';

export default function Layout() {
  const navigate = useNavigate();

  const handleLogout = async () => {
    await api('/logout', { method: 'POST' });
    window.location.href = '/login';
  };

  return (
    <div className="shell">
      <header>
        <h1><Link to="/">LLM Proxy</Link></h1>
        <nav>
          <Link to="/">Dashboard</Link>
          <Link to="/requests">Requests</Link>
          <Link to="/keys">API Keys</Link>
          <Link to="/secrets">Secrets</Link>
          <button onClick={handleLogout}>Log out</button>
        </nav>
      </header>
      <main>
        <Outlet />
      </main>
    </div>
  );
}
