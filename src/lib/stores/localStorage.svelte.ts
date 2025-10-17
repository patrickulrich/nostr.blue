/**
 * Generic store factory for managing localStorage state with Svelte 5 runes
 */
export function createLocalStorage<T>(
  key: string,
  defaultValue: T,
  serializer?: {
    serialize: (value: T) => string;
    deserialize: (value: string) => T;
  }
) {
  const serialize = serializer?.serialize || JSON.stringify;
  const deserialize = serializer?.deserialize || JSON.parse;

  // Initialize state from localStorage
  let initialValue = defaultValue;
  if (typeof window !== 'undefined') {
    try {
      const item = localStorage.getItem(key);
      if (item !== null) {
        initialValue = deserialize(item);
      }
    } catch (error) {
      console.warn(`Failed to load ${key} from localStorage:`, error);
    }
  }

  let state = $state<T>(initialValue);

  // Save to localStorage when state changes
  $effect(() => {
    if (typeof window !== 'undefined') {
      try {
        localStorage.setItem(key, serialize(state));
      } catch (error) {
        console.warn(`Failed to save ${key} to localStorage:`, error);
      }
    }
  });

  // Sync with localStorage changes from other tabs
  $effect(() => {
    if (typeof window === 'undefined') return;

    const handleStorageChange = (e: StorageEvent) => {
      if (e.key === key && e.newValue !== null) {
        try {
          state = deserialize(e.newValue);
        } catch (error) {
          console.warn(`Failed to sync ${key} from localStorage:`, error);
        }
      }
    };

    window.addEventListener('storage', handleStorageChange);
    return () => window.removeEventListener('storage', handleStorageChange);
  });

  return {
    get value() {
      return state;
    },
    set value(newValue: T) {
      state = newValue;
    },
    update(updater: (current: T) => T) {
      state = updater(state);
    }
  };
}
