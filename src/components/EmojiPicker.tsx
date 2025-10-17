import { Button } from '@/components/ui/button';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

interface EmojiPickerProps {
  onEmojiSelect: (emoji: string) => void;
  children: React.ReactNode;
}

const EMOJI_CATEGORIES = {
  smileys: {
    label: '😊',
    emojis: [
      '😀', '😃', '😄', '😁', '😆', '😅', '🤣', '😂',
      '🙂', '🙃', '😉', '😊', '😇', '🥰', '😍', '🤩',
      '😘', '😗', '😚', '😙', '🥲', '😋', '😛', '😜',
      '🤪', '😝', '🤑', '🤗', '🤭', '🤫', '🤔', '🤐',
    ],
  },
  gestures: {
    label: '👍',
    emojis: [
      '👍', '👎', '👊', '✊', '🤛', '🤜', '🤞', '✌️',
      '🤟', '🤘', '👌', '🤌', '🤏', '👈', '👉', '👆',
      '👇', '☝️', '👋', '🤚', '🖐️', '✋', '🖖', '👏',
      '🙌', '👐', '🤲', '🤝', '🙏', '✍️', '💪', '🦾',
    ],
  },
  hearts: {
    label: '❤️',
    emojis: [
      '❤️', '🧡', '💛', '💚', '💙', '💜', '🖤', '🤍',
      '🤎', '💔', '❤️‍🔥', '❤️‍🩹', '💕', '💞', '💓', '💗',
      '💖', '💘', '💝', '💟', '☮️', '✝️', '☪️', '🕉️',
    ],
  },
  nature: {
    label: '🌸',
    emojis: [
      '🌸', '💮', '🏵️', '🌹', '🥀', '🌺', '🌻', '🌼',
      '🌷', '🌱', '🪴', '🌲', '🌳', '🌴', '🌵', '🌾',
      '🌿', '☘️', '🍀', '🍁', '🍂', '🍃', '🪹', '🪺',
    ],
  },
  food: {
    label: '🍕',
    emojis: [
      '🍕', '🍔', '🍟', '🌭', '🍿', '🧂', '🥓', '🥚',
      '🍳', '🧇', '🥞', '🧈', '🍞', '🥐', '🥨', '🥯',
      '🥖', '🫓', '🥪', '🌮', '🌯', '🫔', '🥙', '🧆',
    ],
  },
  activities: {
    label: '⚽',
    emojis: [
      '⚽', '🏀', '🏈', '⚾', '🥎', '🎾', '🏐', '🏉',
      '🥏', '🎱', '🪀', '🏓', '🏸', '🏒', '🏑', '🥍',
      '🏏', '🪃', '🥅', '⛳', '🪁', '🏹', '🎣', '🤿',
    ],
  },
  travel: {
    label: '✈️',
    emojis: [
      '✈️', '🚀', '🛸', '🚁', '🛶', '⛵', '🚤', '🛳️',
      '⛴️', '🛥️', '🚢', '🚂', '🚃', '🚄', '🚅', '🚆',
      '🚇', '🚈', '🚉', '🚊', '🚝', '🚞', '🚋', '🚌',
    ],
  },
  objects: {
    label: '💡',
    emojis: [
      '💡', '🔦', '🕯️', '🪔', '🔥', '🧨', '✨', '🎈',
      '🎉', '🎊', '🎁', '🎀', '🪅', '🪆', '🎏', '🎐',
      '🧧', '🎎', '🏮', '🎑', '🧿', '🪬', '📿', '💎',
    ],
  },
};

/**
 * Emoji picker component with categorized emojis.
 * Displays a popover with tabs for different emoji categories.
 *
 * @param props - Component properties
 * @param props.onEmojiSelect - Callback function when an emoji is selected
 * @param props.children - Trigger element for the popover
 */
export function EmojiPicker({ onEmojiSelect, children }: EmojiPickerProps) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        {children}
      </PopoverTrigger>
      <PopoverContent className="w-80 p-0" align="start">
        <Tabs defaultValue="smileys" className="w-full">
          <TabsList className="w-full justify-start rounded-none border-b bg-transparent p-0">
            {Object.entries(EMOJI_CATEGORIES).map(([key, category]) => (
              <TabsTrigger
                key={key}
                value={key}
                className="data-[state=active]:border-b-2 data-[state=active]:border-primary rounded-none"
              >
                {category.label}
              </TabsTrigger>
            ))}
          </TabsList>
          {Object.entries(EMOJI_CATEGORIES).map(([key, category]) => (
            <TabsContent key={key} value={key} className="p-2 m-0">
              <div className="grid grid-cols-8 gap-1 max-h-64 overflow-y-auto">
                {category.emojis.map((emoji) => (
                  <Button
                    key={emoji}
                    variant="ghost"
                    className="h-10 w-10 p-0 text-xl hover:bg-accent"
                    onClick={() => onEmojiSelect(emoji)}
                  >
                    {emoji}
                  </Button>
                ))}
              </div>
            </TabsContent>
          ))}
        </Tabs>
      </PopoverContent>
    </Popover>
  );
}
