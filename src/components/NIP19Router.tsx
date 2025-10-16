import { useParams, Navigate } from 'react-router-dom';
import { ProfilePage } from '@/pages/ProfilePage';
import { ThreadPage } from '@/pages/ThreadPage';
import { NIP19Page } from '@/pages/NIP19Page';

/**
 * Smart router for NIP-19 identifiers
 * Routes to the appropriate page based on the NIP-19 prefix
 */
export function NIP19Router() {
  const { nip19 } = useParams<{ nip19: string }>();

  if (!nip19) {
    return <Navigate to="/" replace />;
  }

  // Profile identifiers
  if (nip19.startsWith('npub1') || nip19.startsWith('nprofile1')) {
    return <ProfilePage />;
  }

  // Note/Event identifiers
  if (nip19.startsWith('note1') || nip19.startsWith('nevent1')) {
    return <ThreadPage />;
  }

  // Other NIP-19 identifiers (naddr, etc.)
  return <NIP19Page />;
}
