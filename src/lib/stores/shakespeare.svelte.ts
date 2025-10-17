import { signer, pubkey } from '@welshman/app';
import { get } from 'svelte/store';

// Types for Shakespeare API (compatible with OpenAI ChatCompletionMessageParam)
export interface ChatMessage {
	role: 'user' | 'assistant' | 'system';
	content:
		| string
		| Array<{
				type: 'text' | 'image_url';
				text?: string;
				image_url?: {
					url: string;
				};
		  }>;
}

export interface ChatCompletionRequest {
	model: string;
	messages: ChatMessage[];
	stream?: boolean;
	temperature?: number;
	max_tokens?: number;
}

export interface ChatCompletionResponse {
	id: string;
	object: string;
	created: number;
	model: string;
	choices: Array<{
		index: number;
		message: ChatMessage;
		finish_reason: string;
	}>;
	usage: {
		prompt_tokens: number;
		completion_tokens: number;
		total_tokens: number;
	};
}

export interface Model {
	id: string;
	name: string;
	description: string;
	object: string;
	owned_by: string;
	created: number;
	context_window: number;
	pricing: {
		prompt: string;
		completion: string;
	};
}

export interface ModelsResponse {
	object: string;
	data: Model[];
}

// Configuration
const SHAKESPEARE_API_URL = 'https://ai.shakespeare.diy/v1';

/**
 * Create NIP-98 HTTP Auth token for Shakespeare API
 * @param method - HTTP method
 * @param url - Request URL
 * @param body - Optional request body
 * @returns Base64 encoded signed event token
 */
async function createNIP98Token(
	method: string,
	url: string,
	body?: unknown
): Promise<string> {
	const currentSigner = get(signer);
	const currentPubkey = get(pubkey);

	if (!currentSigner || !currentPubkey) {
		throw new Error('User signer is required for NIP-98 authentication');
	}

	// Create the tags array
	const tags: string[][] = [
		['u', url],
		['method', method]
	];

	// Add payload hash for requests with body (following NIP-98 spec)
	if (body && (method === 'POST' || method === 'PUT' || method === 'PATCH')) {
		const bodyString = JSON.stringify(body);
		const encoder = new TextEncoder();
		const data = encoder.encode(bodyString);
		const hashBuffer = await crypto.subtle.digest('SHA-256', data);
		const payloadHash = Array.from(new Uint8Array(hashBuffer))
			.map((b) => b.toString(16).padStart(2, '0'))
			.join('');
		tags.push(['payload', payloadHash]);
	}

	// Create the HTTP request event template
	const template = {
		kind: 27235, // NIP-98 HTTP Auth
		content: '',
		tags,
		created_at: Math.floor(Date.now() / 1000),
		pubkey: currentPubkey
	};

	// Sign the event
	const event = await currentSigner.sign(template);

	// Return the token (base64 encoded event)
	return btoa(JSON.stringify(event));
}

/**
 * Handle API errors with user-friendly messages
 */
async function handleAPIError(response: Response) {
	if (response.status === 401) {
		throw new Error(
			'Authentication failed. Please make sure you are logged in with a Nostr account.'
		);
	} else if (response.status === 402) {
		throw new Error(
			'Insufficient credits. Please add credits to your account to use premium models, or use the free "tybalt" model.'
		);
	} else if (response.status === 400) {
		try {
			const error = await response.json();
			if (error.error?.type === 'invalid_request_error') {
				// Handle specific validation errors
				if (error.error.code === 'minimum_amount_not_met') {
					throw new Error(
						`Minimum credit amount is $${error.error.minimum_amount}. Please increase your payment amount.`
					);
				} else if (error.error.code === 'unsupported_method') {
					throw new Error(
						'Payment method not supported. Please use "stripe" or "lightning".'
					);
				} else if (error.error.code === 'invalid_url') {
					throw new Error('Invalid redirect URL provided for Stripe payment.');
				}
			}
			throw new Error(
				`Invalid request: ${error.error?.message || error.details || error.error || 'Please check your request parameters.'}`
			);
		} catch {
			throw new Error('Invalid request. Please check your parameters and try again.');
		}
	} else if (response.status === 404) {
		throw new Error('Resource not found. Please check the payment ID or try again.');
	} else if (response.status >= 500) {
		throw new Error('Server error. Please try again in a few moments.');
	} else if (!response.ok) {
		try {
			const errorData = await response.json();
			throw new Error(
				`API error: ${errorData.error?.message || errorData.details || errorData.error || response.statusText}`
			);
		} catch {
			throw new Error(
				`Network error: ${response.statusText}. Please check your connection and try again.`
			);
		}
	}
}

/**
 * Send a chat completion request to Shakespeare API
 * @param messages - Array of chat messages
 * @param model - Model to use (default: 'shakespeare')
 * @param options - Additional request options
 * @returns Promise<ChatCompletionResponse>
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { sendChatMessage } from '$lib/stores/shakespeare.svelte';
 *
 *   async function chat() {
 *     try {
 *       const response = await sendChatMessage([
 *         { role: 'user', content: 'Hello!' }
 *       ]);
 *       console.log(response.choices[0].message.content);
 *     } catch (error) {
 *       console.error('Chat failed:', error);
 *     }
 *   }
 * </script>
 * ```
 */
export async function sendChatMessage(
	messages: ChatMessage[],
	model: string = 'shakespeare',
	options?: Partial<ChatCompletionRequest>
): Promise<ChatCompletionResponse> {
	const currentPubkey = get(pubkey);
	if (!currentPubkey) {
		throw new Error('User must be logged in to use AI features');
	}

	try {
		const requestBody: ChatCompletionRequest = {
			model,
			messages,
			...options
		};

		const token = await createNIP98Token(
			'POST',
			`${SHAKESPEARE_API_URL}/chat/completions`,
			requestBody
		);

		const response = await fetch(`${SHAKESPEARE_API_URL}/chat/completions`, {
			method: 'POST',
			headers: {
				Authorization: `Nostr ${token}`,
				'Content-Type': 'application/json'
			},
			body: JSON.stringify(requestBody)
		});

		await handleAPIError(response);
		return await response.json();
	} catch (err) {
		let errorMessage = 'An unexpected error occurred';

		if (err instanceof Error) {
			errorMessage = err.message;
		} else if (typeof err === 'string') {
			errorMessage = err;
		}

		// Add context for common issues
		if (errorMessage.includes('Failed to fetch') || errorMessage.includes('Network')) {
			errorMessage = 'Network error: Please check your internet connection and try again.';
		} else if (errorMessage.includes('signer')) {
			errorMessage =
				'Authentication error: Please make sure you are logged in with a Nostr account that supports signing.';
		}

		throw new Error(errorMessage);
	}
}

/**
 * Send a streaming chat completion request to Shakespeare API
 * @param messages - Array of chat messages
 * @param model - Model to use (default: 'shakespeare')
 * @param onChunk - Callback for each chunk of content
 * @param options - Additional request options
 * @returns Promise<void>
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { sendStreamingMessage } from '$lib/stores/shakespeare.svelte';
 *
 *   let response = $state('');
 *
 *   async function streamChat() {
 *     try {
 *       await sendStreamingMessage(
 *         [{ role: 'user', content: 'Hello!' }],
 *         'shakespeare',
 *         (chunk) => {
 *           response += chunk;
 *         }
 *       );
 *     } catch (error) {
 *       console.error('Stream failed:', error);
 *     }
 *   }
 * </script>
 * ```
 */
export async function sendStreamingMessage(
	messages: ChatMessage[],
	model: string = 'shakespeare',
	onChunk: (chunk: string) => void,
	options?: Partial<ChatCompletionRequest>
): Promise<void> {
	const currentPubkey = get(pubkey);
	if (!currentPubkey) {
		throw new Error('User must be logged in to use AI features');
	}

	try {
		const requestBody: ChatCompletionRequest = {
			model,
			messages,
			stream: true,
			...options
		};

		const token = await createNIP98Token(
			'POST',
			`${SHAKESPEARE_API_URL}/chat/completions`,
			requestBody
		);

		const response = await fetch(`${SHAKESPEARE_API_URL}/chat/completions`, {
			method: 'POST',
			headers: {
				Authorization: `Nostr ${token}`,
				'Content-Type': 'application/json'
			},
			body: JSON.stringify(requestBody)
		});

		await handleAPIError(response);

		if (!response.body) {
			throw new Error('No response body');
		}

		const reader = response.body.getReader();
		const decoder = new TextDecoder();

		try {
			while (true) {
				const { done, value } = await reader.read();
				if (done) break;

				const chunk = decoder.decode(value);
				const lines = chunk.split('\n');

				for (const line of lines) {
					if (line.startsWith('data: ')) {
						const data = line.slice(6);
						if (data === '[DONE]') return;

						try {
							const parsed = JSON.parse(data);
							const content = parsed.choices?.[0]?.delta?.content;
							if (content) {
								onChunk(content);
							}
						} catch {
							// Ignore parsing errors for incomplete chunks
						}
					}
				}
			}
		} finally {
			reader.releaseLock();
		}
	} catch (err) {
		let errorMessage = 'An unexpected error occurred';

		if (err instanceof Error) {
			errorMessage = err.message;
		} else if (typeof err === 'string') {
			errorMessage = err;
		}

		// Add context for common issues
		if (errorMessage.includes('Failed to fetch') || errorMessage.includes('Network')) {
			errorMessage = 'Network error: Please check your internet connection and try again.';
		} else if (errorMessage.includes('signer')) {
			errorMessage =
				'Authentication error: Please make sure you are logged in with a Nostr account that supports signing.';
		}

		throw new Error(errorMessage);
	}
}

/**
 * Get available models from Shakespeare API
 * @returns Promise<ModelsResponse>
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { getAvailableModels } from '$lib/stores/shakespeare.svelte';
 *
 *   async function loadModels() {
 *     try {
 *       const models = await getAvailableModels();
 *       console.log(models.data);
 *     } catch (error) {
 *       console.error('Failed to load models:', error);
 *     }
 *   }
 * </script>
 * ```
 */
export async function getAvailableModels(): Promise<ModelsResponse> {
	const currentPubkey = get(pubkey);
	if (!currentPubkey) {
		throw new Error('User must be logged in to use AI features');
	}

	try {
		const token = await createNIP98Token('GET', `${SHAKESPEARE_API_URL}/models`);

		const response = await fetch(`${SHAKESPEARE_API_URL}/models`, {
			method: 'GET',
			headers: {
				Authorization: `Nostr ${token}`
			}
		});

		await handleAPIError(response);
		return await response.json();
	} catch (err) {
		let errorMessage = 'An unexpected error occurred';

		if (err instanceof Error) {
			errorMessage = err.message;
		} else if (typeof err === 'string') {
			errorMessage = err;
		}

		// Add context for common issues
		if (errorMessage.includes('Failed to fetch') || errorMessage.includes('Network')) {
			errorMessage = 'Network error: Please check your internet connection and try again.';
		} else if (errorMessage.includes('signer')) {
			errorMessage =
				'Authentication error: Please make sure you are logged in with a Nostr account that supports signing.';
		}

		throw new Error(errorMessage);
	}
}

/**
 * Hook-like function for Shakespeare API with reactive state
 * Returns an object with state and functions similar to the original hook
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { useShakespeare } from '$lib/stores/shakespeare.svelte';
 *
 *   const shakespeare = useShakespeare();
 *
 *   async function chat() {
 *     shakespeare.isLoading = true;
 *     try {
 *       const response = await shakespeare.sendChatMessage([
 *         { role: 'user', content: 'Hello!' }
 *       ]);
 *       console.log(response);
 *     } catch (error) {
 *       console.error(error);
 *     }
 *   }
 * </script>
 * ```
 */
export function useShakespeare() {
	let isLoading = $state(false);
	let error = $state<string | null>(null);

	const isAuthenticated = $derived(!!get(pubkey));

	function clearError() {
		error = null;
	}

	async function sendMessage(
		messages: ChatMessage[],
		model: string = 'shakespeare',
		options?: Partial<ChatCompletionRequest>
	): Promise<ChatCompletionResponse> {
		isLoading = true;
		error = null;

		try {
			return await sendChatMessage(messages, model, options);
		} catch (err) {
			error = err instanceof Error ? err.message : 'An unexpected error occurred';
			throw err;
		} finally {
			isLoading = false;
		}
	}

	async function sendStreaming(
		messages: ChatMessage[],
		model: string = 'shakespeare',
		onChunk: (chunk: string) => void,
		options?: Partial<ChatCompletionRequest>
	): Promise<void> {
		isLoading = true;
		error = null;

		try {
			await sendStreamingMessage(messages, model, onChunk, options);
		} catch (err) {
			error = err instanceof Error ? err.message : 'An unexpected error occurred';
			throw err;
		} finally {
			isLoading = false;
		}
	}

	async function getModels(): Promise<ModelsResponse> {
		isLoading = true;
		error = null;

		try {
			return await getAvailableModels();
		} catch (err) {
			error = err instanceof Error ? err.message : 'An unexpected error occurred';
			throw err;
		} finally {
			isLoading = false;
		}
	}

	return {
		get isLoading() {
			return isLoading;
		},
		get error() {
			return error;
		},
		get isAuthenticated() {
			return isAuthenticated;
		},
		sendChatMessage: sendMessage,
		sendStreamingMessage: sendStreaming,
		getAvailableModels: getModels,
		clearError
	};
}
