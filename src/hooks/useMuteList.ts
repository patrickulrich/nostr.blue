import { useLists } from './useLists';

const MUTE_LIST_KIND = 10000;

/**
 * Hook to manage mute list (Kind 10000)
 * Supports muting: pubkeys, hashtags, words, and threads
 */
export function useMuteList() {
  const list = useLists(MUTE_LIST_KIND);

  // Check if a pubkey is muted
  const isMuted = (pubkey: string): boolean => {
    return list.hasItem('p', pubkey);
  };

  // Check if a hashtag is muted
  const isHashtagMuted = (hashtag: string): boolean => {
    return list.hasItem('t', hashtag.toLowerCase());
  };

  // Check if a word is muted
  const isWordMuted = (word: string): boolean => {
    return list.hasItem('word', word.toLowerCase());
  };

  // Check if a thread is muted
  const isThreadMuted = (eventId: string): boolean => {
    return list.hasItem('e', eventId);
  };

  // Mute a pubkey
  const mutePubkey = async (pubkey: string, relay?: string) => {
    return await list.addItem.mutateAsync({ type: 'p', value: pubkey, relay });
  };

  // Unmute a pubkey
  const unmutePubkey = async (pubkey: string) => {
    return await list.removeItem.mutateAsync({ type: 'p', value: pubkey });
  };

  // Toggle mute for a pubkey
  const toggleMutePubkey = async (pubkey: string, relay?: string) => {
    return await list.toggleItem.mutateAsync({ type: 'p', value: pubkey, relay });
  };

  // Mute a hashtag
  const muteHashtag = async (hashtag: string) => {
    return await list.addItem.mutateAsync({ type: 't', value: hashtag.toLowerCase() });
  };

  // Unmute a hashtag
  const unmuteHashtag = async (hashtag: string) => {
    return await list.removeItem.mutateAsync({ type: 't', value: hashtag.toLowerCase() });
  };

  // Mute a word
  const muteWord = async (word: string) => {
    return await list.addItem.mutateAsync({ type: 'word', value: word.toLowerCase() });
  };

  // Unmute a word
  const unmuteWord = async (word: string) => {
    return await list.removeItem.mutateAsync({ type: 'word', value: word.toLowerCase() });
  };

  // Mute a thread
  const muteThread = async (eventId: string) => {
    return await list.addItem.mutateAsync({ type: 'e', value: eventId });
  };

  // Unmute a thread
  const unmuteThread = async (eventId: string) => {
    return await list.removeItem.mutateAsync({ type: 'e', value: eventId });
  };

  return {
    ...list,
    isMuted,
    isHashtagMuted,
    isWordMuted,
    isThreadMuted,
    mutePubkey,
    unmutePubkey,
    toggleMutePubkey,
    muteHashtag,
    unmuteHashtag,
    muteWord,
    unmuteWord,
    muteThread,
    unmuteThread,
  };
}
