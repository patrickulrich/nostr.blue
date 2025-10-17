// Export all stores
export { createLocalStorage } from './localStorage.svelte';
export {
  toast,
  toastSuccess,
  toastError,
  toastInfo,
  toastWarning,
  toastPromise,
  useToast
} from './toast.svelte';
export { useTheme, getTheme, setTheme } from './theme.svelte';
export { appConfig, applyTheme, setupThemeWatcher, type Theme, type AppConfig } from './appStore';
export { createNWCStore, nwcStore, type NWCConnection, type NWCInfo } from './nwc.svelte';
export { useWallet, getWalletStatus, type WalletStatus } from './wallet.svelte';
export { uploadFile, useUploadFile, type UploadResult } from './upload.svelte';

// Re-export Welshman auth utilities
export {
  pubkey,
  session,
  sessions,
  signer,
  currentUser,
  currentPubkey,
  allSessions,
  isLoggedIn,
  otherSessions,
  loginWithExtension,
  loginWithNsec,
  loginWithBunker,
  switchAccount,
  logout,
  publishProfile,
  validateNsec,
  validateBunkerUri,
  hasNostrExtension
} from './auth';

// Author queries
export { fetchAuthor, type AuthorData } from './author.svelte';

// Account management
export {
  fetchAccounts,
  currentUserAccount,
  otherUserAccounts,
  type Account,
  type AccountsData
} from './accounts.svelte';

// Event publishing
export { useNostrPublish, publishNostrEvent } from './publish.svelte';

// Comments
export { fetchComments, type CommentsData } from './comments.svelte';
export { usePostComment } from './postComment.svelte';

// Shakespeare AI
export {
  useShakespeare,
  sendChatMessage,
  sendStreamingMessage,
  getAvailableModels,
  type ChatMessage,
  type ChatCompletionRequest,
  type ChatCompletionResponse,
  type Model,
  type ModelsResponse
} from './shakespeare.svelte';

// Zaps
export { useZaps } from './zaps.svelte';

// App context
export { useAppContext, getAppConfig } from './appContext.svelte';
