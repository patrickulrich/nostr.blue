import { writable, get } from 'svelte/store';
import { browser } from '$app/environment';

/**
 * Generic store factory for managing localStorage state with Svelte stores
 * This version uses writable stores instead of runes to work at module level
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
  if (browser) {
    try {
      const item = localStorage.getItem(key);
      if (item !== null) {
        initialValue = deserialize(item);
      }
    } catch (error) {
      console.warn(`Failed to load ${key} from localStorage:`, error);
    }
  }

  const store = writable<T>(initialValue);

  // Subscribe to store changes and save to localStorage
  if (browser) {
    store.subscribe((value) => {
      try {
        localStorage.setItem(key, serialize(value));
      } catch (error) {
        console.warn(`Failed to save ${key} to localStorage:`, error);
      }
    });

    // Sync with localStorage changes from other tabs
    const handleStorageChange = (e: StorageEvent) => {
      if (e.key === key && e.newValue !== null) {
        try {
          store.set(deserialize(e.newValue));
        } catch (error) {
          console.warn(`Failed to sync ${key} from localStorage:`, error);
        }
      }
    };

    window.addEventListener('storage', handleStorageChange);
  }

  return {
    subscribe: store.subscribe,
    set: store.set,
    update: store.update,
    get value() {
      return get(store);
    },
    set value(newValue: T) {
      store.set(newValue);
    }
  };
}
