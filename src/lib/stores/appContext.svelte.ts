import { appConfig, type AppConfig, type Theme } from './appStore';
import { get } from 'svelte/store';

/**
 * Hook to access and update application configuration
 * Provides a simple interface to the appConfig store
 *
 * @returns Application context with config and update methods
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { useAppContext } from '$lib/stores/appContext.svelte';
 *
 *   const { config, updateTheme, updateRelayUrl } = useAppContext();
 *
 *   let currentTheme = $derived($config.theme);
 *   let relayUrl = $derived($config.relayUrl);
 * </script>
 *
 * <select bind:value={currentTheme} onchange={() => updateTheme(currentTheme)}>
 *   <option value="light">Light</option>
 *   <option value="dark">Dark</option>
 *   <option value="system">System</option>
 * </select>
 * ```
 */
export function useAppContext() {
	return {
		config: appConfig,
		updateTheme: (theme: Theme) => appConfig.updateTheme(theme),
		updateRelayUrl: (relayUrl: string) => appConfig.updateRelayUrl(relayUrl),
		updateConfig: (config: AppConfig) => appConfig.set(config)
	};
}

/**
 * Get the current app config value
 * Useful for non-reactive access to config
 */
export function getAppConfig(): AppConfig {
	return get(appConfig);
}
