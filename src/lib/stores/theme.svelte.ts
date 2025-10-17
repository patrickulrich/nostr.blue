import { appConfig, type Theme } from './appStore';
import { get } from 'svelte/store';

/**
 * Theme utilities for Svelte 5
 * Provides a similar API to the React useTheme hook
 */
export function useTheme() {
  return {
    get theme(): Theme {
      return get(appConfig).theme;
    },
    setTheme(theme: Theme) {
      appConfig.updateTheme(theme);
    }
  };
}

// For direct access outside of reactive contexts
export function getTheme(): Theme {
  return get(appConfig).theme;
}

export function setTheme(theme: Theme) {
  appConfig.updateTheme(theme);
}
