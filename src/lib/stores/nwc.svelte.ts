import { createLocalStorage } from './localStorage.svelte';
import { toast } from './toast.svelte';
import { LN } from '@getalby/sdk';

export interface NWCConnection {
  connectionString: string;
  alias?: string;
  isConnected: boolean;
  client?: LN;
}

export interface NWCInfo {
  alias?: string;
  color?: string;
  pubkey?: string;
  network?: string;
  methods?: string[];
  notifications?: string[];
}

/**
 * NWC (Nostr Wallet Connect) store for managing wallet connections
 * Converted from React useNWC hook to Svelte 5
 */
export function createNWCStore() {
  // Storage for connections and active connection
  const connections = createLocalStorage<NWCConnection[]>('nwc-connections', []);
  const activeConnection = createLocalStorage<string | null>('nwc-active-connection', null);

  let connectionInfo = $state<Record<string, NWCInfo>>({});

  // Add new connection
  async function addConnection(uri: string, alias?: string): Promise<boolean> {
    const parseNWCUri = (uri: string): { connectionString: string } | null => {
      try {
        if (!uri.startsWith('nostr+walletconnect://') && !uri.startsWith('nostrwalletconnect://')) {
          console.error('Invalid NWC URI protocol:', { protocol: uri.split('://')[0] });
          return null;
        }
        return { connectionString: uri };
      } catch (error) {
        console.error('Failed to parse NWC URI:', error);
        return null;
      }
    };

    const parsed = parseNWCUri(uri);
    if (!parsed) {
      toast({
        title: 'Invalid NWC URI',
        description: 'Please check the connection string and try again.',
        variant: 'destructive',
      });
      return false;
    }

    const existingConnection = connections.value.find(c => c.connectionString === parsed.connectionString);
    if (existingConnection) {
      toast({
        title: 'Connection already exists',
        description: 'This wallet is already connected.',
        variant: 'destructive',
      });
      return false;
    }

    try {
      let timeoutId: ReturnType<typeof setTimeout> | undefined;
      const testPromise = new Promise((resolve, reject) => {
        try {
          const client = new LN(parsed.connectionString);
          resolve(client);
        } catch (error) {
          reject(error);
        }
      });
      const timeoutPromise = new Promise<never>((_, reject) => {
        timeoutId = setTimeout(() => reject(new Error('Connection test timeout')), 10000);
      });

      try {
        await Promise.race([testPromise, timeoutPromise]) as LN;
        if (timeoutId) clearTimeout(timeoutId);
      } catch (error) {
        if (timeoutId) clearTimeout(timeoutId);
        throw error;
      }

      const connection: NWCConnection = {
        connectionString: parsed.connectionString,
        alias: alias || 'NWC Wallet',
        isConnected: true,
      };

      connectionInfo = {
        ...connectionInfo,
        [parsed.connectionString]: {
          alias: connection.alias,
          methods: ['pay_invoice'],
        },
      };

      connections.value = [...connections.value, connection];

      if (connections.value.length === 1 || !activeConnection.value) {
        activeConnection.value = parsed.connectionString;
      }

      toast({
        title: 'Wallet connected',
        description: `Successfully connected to ${connection.alias}.`,
      });

      return true;
    } catch (error) {
      console.error('NWC connection failed:', error);
      const errorMessage = error instanceof Error ? error.message : 'Unknown error occurred';

      toast({
        title: 'Connection failed',
        description: `Could not connect to the wallet: ${errorMessage}`,
        variant: 'destructive',
      });
      return false;
    }
  }

  // Remove connection
  function removeConnection(connectionString: string) {
    const filtered = connections.value.filter(c => c.connectionString !== connectionString);
    connections.value = filtered;

    if (activeConnection.value === connectionString) {
      const newActive = filtered.length > 0 ? filtered[0].connectionString : null;
      activeConnection.value = newActive;
    }

    const newInfo = { ...connectionInfo };
    delete newInfo[connectionString];
    connectionInfo = newInfo;

    toast({
      title: 'Wallet disconnected',
      description: 'The wallet connection has been removed.',
    });
  }

  // Get active connection
  function getActiveConnection(): NWCConnection | null {
    if (!activeConnection.value && connections.value.length > 0) {
      activeConnection.value = connections.value[0].connectionString;
      return connections.value[0];
    }

    if (!activeConnection.value) return null;

    const found = connections.value.find(c => c.connectionString === activeConnection.value);
    return found || null;
  }

  // Set active connection
  function setActiveConnection(connectionString: string) {
    activeConnection.value = connectionString;
  }

  // Send payment using the SDK
  async function sendPayment(
    connection: NWCConnection,
    invoice: string
  ): Promise<{ preimage: string }> {
    if (!connection.connectionString) {
      throw new Error('Invalid connection: missing connection string');
    }

    let client: LN;
    try {
      client = new LN(connection.connectionString);
    } catch (error) {
      console.error('Failed to create NWC client:', error);
      throw new Error(`Failed to create NWC client: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }

    try {
      let timeoutId: ReturnType<typeof setTimeout> | undefined;
      const timeoutPromise = new Promise<never>((_, reject) => {
        timeoutId = setTimeout(() => reject(new Error('Payment timeout after 15 seconds')), 15000);
      });

      const paymentPromise = client.pay(invoice);

      try {
        const response = await Promise.race([paymentPromise, timeoutPromise]) as { preimage: string };
        if (timeoutId) clearTimeout(timeoutId);
        return response;
      } catch (error) {
        if (timeoutId) clearTimeout(timeoutId);
        throw error;
      }
    } catch (error) {
      console.error('NWC payment failed:', error);

      if (error instanceof Error) {
        if (error.message.includes('timeout')) {
          throw new Error('Payment timed out. Please try again.');
        } else if (error.message.includes('insufficient')) {
          throw new Error('Insufficient balance in connected wallet.');
        } else if (error.message.includes('invalid')) {
          throw new Error('Invalid invoice or connection. Please check your wallet.');
        } else {
          throw new Error(`Payment failed: ${error.message}`);
        }
      }

      throw new Error('Payment failed with unknown error');
    }
  }

  return {
    get connections() {
      return connections.value;
    },
    get activeConnection() {
      return activeConnection.value;
    },
    get connectionInfo() {
      return connectionInfo;
    },
    addConnection,
    removeConnection,
    setActiveConnection,
    getActiveConnection,
    sendPayment,
  };
}

// Create singleton instance
export const nwcStore = createNWCStore();
