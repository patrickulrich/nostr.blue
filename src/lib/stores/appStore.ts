import { writable, get } from 'svelte/store';
import { z } from 'zod';
import { browser } from '$app/environment';

export type Theme = "dark" | "light" | "system";

export interface AppConfig {
  theme: Theme;
  relayUrl: string;
}

export interface PresetRelay {
  name: string;
  url: string;
}

// Zod schema for AppConfig validation
const AppConfigSchema = z.object({
  theme: z.enum(['dark', 'light', 'system']),
  relayUrl: z.string().url(),
}) satisfies z.ZodType<AppConfig>;

// Default configuration
export const defaultConfig: AppConfig = {
  theme: "light",
  relayUrl: "wss://relay.ditto.pub",
};

// Preset relays
export const presetRelays: PresetRelay[] = [
  { url: 'wss://relay.ditto.pub', name: 'Ditto' },
  { url: 'wss://relay.nostr.band', name: 'Nostr.Band' },
  { url: 'wss://relay.damus.io', name: 'Damus' },
  { url: 'wss://relay.primal.net', name: 'Primal' },
];

// Storage key for app config
const STORAGE_KEY = 'nostr:app-config';

// Load config from localStorage
function loadConfig(): AppConfig {
  if (!browser) return defaultConfig;

  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      const validated = AppConfigSchema.partial().parse(parsed);
      return { ...defaultConfig, ...validated };
    }
  } catch (e) {
    console.error('Failed to load app config:', e);
  }

  return defaultConfig;
}

// Save config to localStorage
function saveConfig(config: AppConfig) {
  if (!browser) return;

  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
  } catch (e) {
    console.error('Failed to save app config:', e);
  }
}

// Create the app config store
function createAppConfigStore() {
  const { subscribe, set, update } = writable<AppConfig>(loadConfig());

  return {
    subscribe,
    set: (value: AppConfig) => {
      saveConfig(value);
      set(value);
    },
    update: (updater: (current: AppConfig) => AppConfig) => {
      update((current) => {
        const newConfig = updater(current);
        saveConfig(newConfig);
        return newConfig;
      });
    },
    updateTheme: (theme: Theme) => {
      update((current) => {
        const newConfig = { ...current, theme };
        saveConfig(newConfig);
        applyTheme(theme);
        return newConfig;
      });
    },
    updateRelayUrl: (relayUrl: string) => {
      update((current) => {
        const newConfig = { ...current, relayUrl };
        saveConfig(newConfig);
        return newConfig;
      });
    },
  };
}

// Apply theme to document
export function applyTheme(theme: Theme) {
  if (!browser) return;

  const root = window.document.documentElement;
  root.classList.remove('light', 'dark');

  if (theme === 'system') {
    const systemTheme = window.matchMedia('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light';
    root.classList.add(systemTheme);
  } else {
    root.classList.add(theme);
  }
}

// Setup theme watcher for system theme changes
export function setupThemeWatcher() {
  if (!browser) return;

  const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');

  const handleChange = () => {
    const currentConfig = get(appConfig);
    if (currentConfig.theme === 'system') {
      applyTheme('system');
    }
  };

  mediaQuery.addEventListener('change', handleChange);

  return () => mediaQuery.removeEventListener('change', handleChange);
}

// Export the store
export const appConfig = createAppConfigStore();
