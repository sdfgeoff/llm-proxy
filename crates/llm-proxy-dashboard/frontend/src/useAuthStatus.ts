import { useState, useEffect, useRef } from 'react';
import { useNavigate } from 'react-router-dom';

export type AuthStatus = 'loading' | 'needs_setup' | 'unauthenticated' | 'authenticated';

export default function useAuthStatus(): AuthStatus {
  const [status, setStatus] = useState<AuthStatus>('loading');
  const navigate = useNavigate();
  const cancelledRef = useRef(false);

  useEffect(() => {
    cancelledRef.current = false;

    fetch('/api/auth/status', { credentials: 'same-origin' })
      .then((res) => res.json() as Promise<{ status: string }>)
      .then(({ status }) => {
        if (!cancelledRef.current) setStatus(status as AuthStatus);
      })
      .catch(() => {
        if (!cancelledRef.current) setStatus('loading');
      });

    return () => { cancelledRef.current = true; };
  }, [navigate]);

  return status;
}
