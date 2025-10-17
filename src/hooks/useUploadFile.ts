import { useMutation } from "@tanstack/react-query";
import { BlossomUploader } from '@nostrify/nostrify/uploaders';

import { useCurrentUser } from "./useCurrentUser";

/**
 * Hook for uploading files to Blossom servers (NIP-96).
 * Requires user to be logged in for authentication.
 *
 * @returns Mutation result for file upload operation
 */
export function useUploadFile() {
  const { user } = useCurrentUser();

  return useMutation({
    mutationFn: async (file: File) => {
      if (!user) {
        throw new Error('Must be logged in to upload files');
      }

      const uploader = new BlossomUploader({
        servers: [
          'https://blossom.primal.net/',
        ],
        signer: user.signer,
      });

      const tags = await uploader.upload(file);
      return tags;
    },
  });
}