import { Routes, Route, Navigate } from 'react-router-dom';
import Layout from './Layout';
import Setup from './pages/Setup';
import Login from './pages/Login';
import Dashboard from './pages/Dashboard';
import Requests from './pages/Requests';
import RequestDetail from './pages/RequestDetail';
import Keys from './pages/Keys';
import Secrets from './pages/Secrets';
import useAuthStatus from './useAuthStatus';

function AuthGate({ children }: { children: React.ReactNode }) {
  const status = useAuthStatus();

  if (status === 'loading') {
    return (
      <div className="loading">
        <span className="spinner" />
        Checking auth...
      </div>
    );
  }
  if (status === 'needs_setup') return <Navigate to="/setup" replace />;
  if (status === 'unauthenticated') return <Navigate to="/login" replace />;

  return <>{children}</>;
}

export default function App() {
  return (
    <Routes>
      <Route path="/setup" element={<Setup />} />
      <Route path="/login" element={<Login />} />
      <Route path="/" element={<AuthGate><Layout /></AuthGate>}>
        <Route index element={<Dashboard />} />
        <Route path="requests" element={<Requests />} />
        <Route path="requests/:id" element={<RequestDetail />} />
        <Route path="keys" element={<Keys />} />
        <Route path="secrets" element={<Secrets />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
