import { toast as sonnerToast } from 'svelte-sonner';

/**
 * Toast utility wrapper for svelte-sonner
 * Provides a similar API to the React useToast hook
 */

export interface ToastOptions {
  title?: string;
  description?: string;
  variant?: 'default' | 'destructive';
  duration?: number;
}

export function toast(options: ToastOptions) {
  const { title, description, variant, duration } = options;

  // Combine title and description for sonner
  const message = title || '';
  const opts: { description?: string; duration: number } = {
    description,
    duration: duration || 4000,
  };

  // Use error toast for destructive variant
  if (variant === 'destructive') {
    return sonnerToast.error(message, opts);
  }

  return sonnerToast(message, opts);
}

// Also export individual toast methods for convenience
export const toastSuccess = (message: string, description?: string, duration?: number) => {
  return sonnerToast.success(message, { description, duration });
};

export const toastError = (message: string, description?: string, duration?: number) => {
  return sonnerToast.error(message, { description, duration });
};

export const toastInfo = (message: string, description?: string, duration?: number) => {
  return sonnerToast.info(message, { description, duration });
};

export const toastWarning = (message: string, description?: string, duration?: number) => {
  return sonnerToast.warning(message, { description, duration });
};

export const toastPromise = <T,>(
  promise: Promise<T>,
  options: {
    loading: string;
    success: string | ((data: T) => string);
    error: string | ((error: unknown) => string);
  }
) => {
  return sonnerToast.promise(promise, options);
};

// For compatibility with React useToast pattern
export function useToast() {
  return {
    toast,
    toastSuccess,
    toastError,
    toastInfo,
    toastWarning,
    toastPromise,
  };
}
