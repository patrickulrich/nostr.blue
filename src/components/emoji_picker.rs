use dioxus::prelude::*;
use std::collections::HashSet;
use crate::stores::emoji_store::{
    CUSTOM_EMOJIS, EMOJI_SETS, RECENT_EMOJIS,
    CustomEmojisStoreStoreExt, EmojiSetsStoreStoreExt,
    save_recent_emoji,
};

#[derive(Props, Clone, PartialEq)]
pub struct EmojiPickerProps {
    pub on_emoji_selected: EventHandler<String>,
    #[props(default = false)]
    pub icon_only: bool,
}

/// Comprehensive emoji categories with extensive emoji coverage
const EMOJI_CATEGORIES: &[(&str, &[&str])] = &[
    // Smileys & Emotion (expanded)
    ("😀 Smileys", &[
        "😀", "😃", "😄", "😁", "😆", "😅", "🤣", "😂", "🙂", "🙃", "🫠", "😉", "😊", "😇",
        "🥰", "😍", "🤩", "😘", "😗", "☺️", "😚", "😙", "🥲", "😋", "😛", "😜", "🤪", "😝",
        "🤑", "🤗", "🤭", "🫢", "🫣", "🤫", "🤔", "🫡", "🤐", "🤨", "😐", "😑", "😶", "🫥",
        "😶‍🌫️", "😏", "😒", "🙄", "😬", "😮‍💨", "🤥", "🫨", "😌", "😔", "😪", "🤤", "😷",
        "🤒", "🤕", "🤢", "🤮", "🤧", "🥵", "🥶", "🥴", "😵", "😵‍💫", "🤠", "🥳", "🥸", "😎"
    ]),

    // Love & Hearts (expanded)
    ("❤️ Love", &[
        "❤️", "🧡", "💛", "💚", "💙", "💜", "🤎", "🖤", "🤍", "💔", "❤️‍🔥", "❤️‍🩹",
        "❣️", "💕", "💞", "💓", "💗", "💖", "💘", "💝", "💟", "☮️", "✝️", "☪️", "🕉",
        "☸️", "✡️", "🔯", "🕎", "☯️", "☦️", "🛐", "⛎", "♈", "♉", "♊", "♋", "♌", "♍"
    ]),

    // Hand Gestures (expanded)
    ("👍 Hands", &[
        "👍", "👎", "👌", "✌️", "🤞", "🫰", "🤟", "🤘", "🤙", "👈", "👉", "👆", "🖕", "👇",
        "☝️", "🫵", "👋", "🤚", "🖐", "✋", "🖖", "🫱", "🫲", "🫳", "🫴", "👏", "🙌",
        "👐", "🤲", "🤝", "🙏", "🫂", "✍️", "💅", "🤳", "💪", "🦾", "🦿", "🦵", "🦶", "👂", "🦻",
        "🤦‍♀️", "🤦", "🤦‍♂️", "🤷‍♀️", "🤷", "🤷‍♂️", "🙅‍♀️", "🙅", "🙅‍♂️", "🙆‍♀️", "🙆", "🙆‍♂️",
        "💁‍♀️", "💁", "💁‍♂️", "🙋‍♀️", "🙋", "🙋‍♂️", "🧏‍♀️", "🧏", "🧏‍♂️", "🙇‍♀️", "🙇", "🙇‍♂️",
        "🤏", "👌", "🤌", "🤞", "🤜", "🤛", "👊", "✊", "👃", "🧠", "🫀", "🫁", "🦷", "🦴",
        "👀", "👁️", "👅", "👄", "🫦", "💋"
    ]),

    // Emotions & Faces (expanded)
    ("😢 Emotions", &[
        "🥺", "🥹", "😢", "😭", "😤", "😠", "😡", "🤬", "🤯", "😳", "🥵", "🥶", "😱", "😨",
        "😰", "😥", "😓", "🫗", "🤗", "🫣", "😖", "😣", "😞", "😟", "😔", "😕", "🙁", "☹️",
        "😩", "😫", "🥱", "😴", "😪", "🤤", "😮", "😦", "😧", "😯", "😲", "🤐", "😵", "😵‍💫",
        "🤓", "🧐", "😈", "👿", "👹", "👺", "💀", "☠️", "👻", "👽", "👾", "🤖", "💩", "😺",
        "😸", "😹", "😻", "😼", "😽", "🙀", "😿", "😾"
    ]),

    // People & Body (new)
    ("👤 People", &[
        "👶", "👧", "🧒", "👦", "👩", "🧑", "👨", "👩‍🦱", "🧑‍🦱", "👨‍🦱", "👩‍🦰", "🧑‍🦰",
        "👨‍🦰", "👱‍♀️", "👱", "👱‍♂️", "👩‍🦳", "🧑‍🦳", "👨‍🦳", "👩‍🦲", "🧑‍🦲", "👨‍🦲",
        "🧔‍♀️", "🧔", "🧔‍♂️", "👵", "🧓", "👴", "👲", "👳‍♀️", "👳", "👳‍♂️", "🧕", "👮‍♀️",
        "👮", "👮‍♂️", "👷‍♀️", "👷", "👷‍♂️", "💂‍♀️", "💂", "💂‍♂️", "🕵️‍♀️", "🕵️", "🕵️‍♂️",
        "👩‍⚕️", "🧑‍⚕️", "👨‍⚕️", "👩‍🌾", "🧑‍🌾", "👨‍🌾", "👩‍🍳", "🧑‍🍳", "👨‍🍳", "👩‍🎓",
        "🧑‍🎓", "👨‍🎓", "👩‍🎤", "🧑‍🎤", "👨‍🎤", "👩‍🏫", "🧑‍🏫", "👨‍🏫", "👩‍🏭", "🧑‍🏭",
        "👨‍🏭", "👩‍💻", "🧑‍💻", "👨‍💻", "👩‍💼", "🧑‍💼", "👨‍💼", "👩‍🔧", "🧑‍🔧", "👨‍🔧",
        "👩‍🔬", "🧑‍🔬", "👨‍🔬", "👩‍🎨", "🧑‍🎨", "👨‍🎨", "👩‍🚒", "🧑‍🚒", "👨‍🚒", "👩‍✈️",
        "🧑‍✈️", "👨‍✈️", "👩‍🚀", "🧑‍🚀", "👨‍🚀", "👩‍⚖️", "🧑‍⚖️", "👨‍⚖️", "👰‍♀️", "👰",
        "👰‍♂️", "🤵‍♀️", "🤵", "🤵‍♂️", "👸", "🤴", "🥷", "🦸‍♀️", "🦸", "🦸‍♂️", "🦹‍♀️", "🦹",
        "🦹‍♂️", "🧙‍♀️", "🧙", "🧙‍♂️", "🧚‍♀️", "🧚", "🧚‍♂️", "🧛‍♀️", "🧛", "🧛‍♂️", "🧜‍♀️",
        "🧜", "🧜‍♂️", "🧝‍♀️", "🧝", "🧝‍♂️", "🧞‍♀️", "🧞", "🧞‍♂️", "🧟‍♀️", "🧟", "🧟‍♂️"
    ]),

    // Animals & Nature (expanded)
    ("🐶 Animals", &[
        "🐶", "🐕", "🦮", "🐕‍🦺", "🐩", "🐺", "🦊", "🦝", "🐱", "🐈", "🐈‍⬛", "🦁", "🐯",
        "🐅", "🐆", "🐴", "🫎", "🫏", "🐎", "🦄", "🦓", "🦌", "🦬", "🐮", "🐂", "🐃", "🐄",
        "🐷", "🐖", "🐗", "🐽", "🐏", "🐑", "🐐", "🐪", "🐫", "🦙", "🦒", "🐘", "🦣", "🦏",
        "🦛", "🐭", "🐁", "🐀", "🐹", "🐰", "🐇", "🐿️", "🦫", "🦔", "🦇", "🐻", "🐻‍❄️",
        "🐨", "🐼", "🦥", "🦦", "🦨", "🦘", "🦡", "🐾", "🦃", "🐔", "🐓", "🐣", "🐤", "🐥",
        "🐦", "🐧", "🕊️", "🦅", "🦆", "🦢", "🦉", "🦤", "🪶", "🦩", "🦚", "🦜", "🐸", "🐊",
        "🐢", "🦎", "🐍", "🐲", "🐉", "🦕", "🦖", "🐳", "🐋", "🐬", "🦭", "🐟", "🐠", "🐡",
        "🦈", "🐙", "🐚", "🪸", "🦀", "🦞", "🦐", "🦑", "🐌", "🦋", "🐛", "🐜", "🐝", "🪲",
        "🐞", "🦗", "🪳", "🕷️", "🕸️", "🦂", "🦟", "🪰", "🪱", "🦠"
    ]),

    // Food & Drink (new)
    ("🍕 Food", &[
        "🍏", "🍎", "🍐", "🍊", "🍋", "🍌", "🍉", "🍇", "🍓", "🫐", "🍈", "🍒", "🍑", "🥭",
        "🍍", "🥥", "🥝", "🍅", "🍆", "🥑", "🥦", "🥬", "🥒", "🌶️", "🫑", "🌽", "🥕", "🫒",
        "🧄", "🧅", "🥔", "🍠", "🥐", "🥯", "🍞", "🥖", "🥨", "🧀", "🥚", "🍳", "🧈", "🥞",
        "🧇", "🥓", "🥩", "🍗", "🍖", "🦴", "🌭", "🍔", "🍟", "🍕", "🫓", "🥪", "🥙", "🧆",
        "🌮", "🌯", "🫔", "🥗", "🥘", "🫕", "🥫", "🍝", "🍜", "🍲", "🍛", "🍣", "🍱", "🥟",
        "🦪", "🍤", "🍙", "🍚", "🍘", "🍥", "🥠", "🥮", "🍢", "🍡", "🍧", "🍨", "🍦", "🥧",
        "🧁", "🍰", "🎂", "🍮", "🍭", "🍬", "🍫", "🍿", "🍩", "🍪", "🌰", "🥜", "🫘", "🍯"
    ]),

    // Activities & Sports (expanded)
    ("⚽ Activity", &[
        "⚽", "🏀", "🏈", "⚾", "🥎", "🎾", "🏐", "🏉", "🥏", "🎱", "🪀", "🏓", "🏸", "🏒",
        "🏑", "🥍", "🏏", "🪃", "🥅", "⛳", "🪁", "🏹", "🎣", "🤿", "🥊", "🥋", "🎽", "🛹",
        "🛼", "🛷", "⛸️", "🥌", "🎿", "⛷️", "🏂", "🪂", "🏋️‍♀️", "🏋️", "🏋️‍♂️", "🤼‍♀️", "🤼",
        "🤼‍♂️", "🤸‍♀️", "🤸", "🤸‍♂️", "⛹️‍♀️", "⛹️", "⛹️‍♂️", "🤺", "🤾‍♀️", "🤾", "🤾‍♂️",
        "🏌️‍♀️", "🏌️", "🏌️‍♂️", "🏇", "🧘‍♀️", "🧘", "🧘‍♂️", "🏄‍♀️", "🏄", "🏄‍♂️", "🏊‍♀️",
        "🏊", "🏊‍♂️", "🤽‍♀️", "🤽", "🤽‍♂️", "🚣‍♀️", "🚣", "🚣‍♂️", "🧗‍♀️", "🧗", "🧗‍♂️",
        "🚴‍♀️", "🚴", "🚴‍♂️", "🚵‍♀️", "🚵", "🚵‍♂️", "🤹‍♀️", "🤹", "🤹‍♂️", "🧖‍♀️", "🧖",
        "🧖‍♂️", "🧑‍🦯", "🧑‍🦼", "🧑‍🦽", "🎪", "🎭", "🎨", "🎬", "🎤", "🎧", "🎼", "🎹", "🥁",
        "🎷", "🎺", "🪗", "🎸", "🪕", "🎻", "🎲", "♟️", "🎯", "🎳", "🎮", "🎰", "🧩"
    ]),

    // Travel & Places (new)
    ("✈️ Travel", &[
        "🚗", "🚕", "🚙", "🚌", "🚎", "🏎️", "🚓", "🚑", "🚒", "🚐", "🛻", "🚚", "🚛", "🚜",
        "🦯", "🦽", "🦼", "🛴", "🚲", "🛵", "🏍️", "🛺", "🚨", "🚔", "🚍", "🚘", "🚖", "🚡",
        "🚠", "🚟", "🚃", "🚋", "🚞", "🚝", "🚄", "🚅", "🚈", "🚂", "🚆", "🚇", "🚊", "🚉",
        "✈️", "🛫", "🛬", "🛩️", "💺", "🛰️", "🚀", "🛸", "🚁", "🛶", "⛵", "🚤", "🛥️", "🛳️",
        "⛴️", "🚢", "⚓", "🪝", "⛽", "🚧", "🚦", "🚥", "🚏", "🗺️", "🗿", "🗽", "🗼", "🏰",
        "🏯", "🏟️", "🎡", "🎢", "🎠", "⛲", "⛱️", "🏖️", "🏝️", "🏜️", "🌋", "⛰️", "🏔️", "🗻",
        "🏕️", "⛺", "🛖", "🏠", "🏡", "🏘️", "🏚️", "🏗️", "🏭", "🏢", "🏬", "🏣", "🏤", "🏥",
        "🏦", "🏨", "🏪", "🏫", "🏩", "💒", "🏛️", "⛪", "🕌", "🕍", "🛕", "🕋", "⛩️", "🛤️", "🛣️"
    ]),

    // Objects (new)
    ("💡 Objects", &[
        "⌚", "📱", "📲", "💻", "⌨️", "🖥️", "🖨️", "🖱️", "🖲️", "🕹️", "🗜️", "💾", "💿", "📀",
        "📼", "📷", "📸", "📹", "🎥", "📽️", "🎞️", "📞", "☎️", "📟", "📠", "📺", "📻", "🎙️",
        "🎚️", "🎛️", "🧭", "⏱️", "⏲️", "⏰", "🕰️", "⌛", "⏳", "📡", "🔋", "🪫", "🔌", "💡",
        "🔦", "🕯️", "🪔", "🧯", "🛢️", "💸", "💵", "💴", "💶", "💷", "🪙", "💰", "💳", "🧾",
        "💎", "⚖️", "🪜", "🧰", "🪛", "🔧", "🔨", "⚒️", "🛠️", "⛏️", "🪚", "🔩", "⚙️", "🪤",
        "🧱", "⛓️", "🧲", "🔫", "💣", "🧨", "🪓", "🔪", "🗡️", "⚔️", "🛡️", "🚬", "⚰️", "🪦",
        "⚱️", "🏺", "🔮", "📿", "🧿", "🪬", "💈", "⚗️", "🔭", "🔬", "🕳️", "🩻", "🩹", "🩺",
        "💊", "💉", "🩸", "🧬", "🦷", "🦴", "🧹", "🪠", "🧺", "🧻", "🚽", "🚿", "🛁", "🪥",
        "🪒", "🧴", "🧽", "🪣", "🧼", "🪧", "🔑", "🗝️", "🚪", "🪑", "🛋️", "🛏️", "🖼️", "🪞",
        "🪟", "🛍️", "🛒", "🎁", "🎈", "🎏", "🎀", "🪄", "🪅", "🎊", "🎉", "🎎", "🏮", "🎐",
        "🧧", "✉️", "📩", "📨", "📧", "💌", "📥", "📤", "📦", "🏷️", "🪪", "📪", "📫", "📬",
        "📭", "📮", "📯", "📜", "📃", "📄", "📑", "🧾", "📊", "📈", "📉", "🗒️", "🗓️", "📆",
        "📅", "🗑️", "📇", "🗃️", "🗳️", "🗄️", "📋", "📁", "📂", "🗂️", "🗞️", "📰", "📓", "📔",
        "📒", "📕", "📗", "📘", "📙", "📚", "📖", "🔖", "🧷", "🔗", "📎", "🖇️", "📐", "📏",
        "🧮", "📌", "📍", "✂️", "🖊️", "🖋️", "✒️", "🖌️", "🖍️", "📝", "✏️", "🔍", "🔎", "🔏",
        "🔐", "🔒", "🔓"
    ]),

    // Symbols (expanded)
    ("⚡ Symbols", &[
        "⚡", "🔥", "💯", "✅", "☑️", "✔️", "❌", "❎", "➕", "➖", "➗", "✖️", "🟰", "💲",
        "💱", "™️", "©️", "®️", "〰️", "➰", "➿", "🔚", "🔙", "🔛", "🔝", "🔜", "✳️", "✴️",
        "❇️", "‼️", "⁉️", "❓", "❔", "❕", "❗", "〽️", "⚠️", "🚸", "🔱", "⚜️", "🔰", "♻️",
        "⭐", "🌟", "✨", "⚡", "💫", "💥", "💢", "💦", "💨", "🕊️", "🚀", "💎", "🔔", "🔕",
        "🔁", "📤", "🔴", "🟠", "🟡", "🟢", "🔵", "🟣", "🟤", "⚫", "⚪", "🟥", "🟧", "🟨",
        "🟩", "🟦", "🟪", "🟫", "⬛", "⬜", "◼️", "◻️", "◾", "◽", "▪️", "▫️", "🔶", "🔷",
        "🔸", "🔹", "🔺", "🔻", "💠", "🔘", "🔳", "🔲", "🏁", "🚩", "🎌", "🏴", "🏳️", "🌐",
        "🆔", "⚛️", "🕉️", "✡️", "☸️", "☯️", "✝️", "☦️", "☪️", "☮️", "🕎", "🔯", "♈", "♉",
        "♊", "♋", "♌", "♍", "♎", "♏", "♐", "♑", "♒", "♓", "⛎", "🔀", "🔁", "🔂", "▶️",
        "⏩", "⏭️", "⏯️", "◀️", "⏪", "⏮️", "🔼", "⏫", "🔽", "⏬", "⏸️", "⏹️", "⏺️", "⏏️",
        "🎦", "🔅", "🔆", "📶", "🛜", "📳", "📴", "♀️", "♂️", "⚧️", "✖️", "➕", "➖", "➗",
        "🟰", "♾️", "‼️", "⁉️", "❓", "❔", "❕", "❗", "〰️", "💱", "💲", "⚕️", "♻️", "⚜️",
        "🔱", "📛", "🔰", "⭕", "✅", "☑️", "✔️", "❌", "❎", "➰", "➿", "〽️", "✳️", "✴️",
        "❇️", "©️", "®️", "™️"
    ]),

    // Nature & Weather (new)
    ("🌸 Nature", &[
        "💐", "🌸", "💮", "🪷", "🏵️", "🌹", "🥀", "🌺", "🌻", "🌼", "🌷", "🌱", "🪴", "🌲",
        "🌳", "🌴", "🌵", "🌾", "🌿", "☘️", "🍀", "🍁", "🍂", "🍃", "🪹", "🪺", "🍄", "🌰",
        "🌍", "🌎", "🌏", "🌐", "🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘", "🌙", "🌚",
        "🌛", "🌜", "🌝", "🌞", "⭐", "🌟", "🌠", "🌌", "☁️", "⛅", "⛈️", "🌤️", "🌥️", "🌦️",
        "🌧️", "🌨️", "🌩️", "🌪️", "🌫️", "🌬️", "🌀", "🌈", "🌂", "☂️", "☔", "⛱️", "⚡", "❄️",
        "☃️", "⛄", "☄️", "🔥", "💧", "🌊", "🎃", "🎄", "🎆", "🎇", "🧨", "✨", "🎈", "🎉",
        "🎊", "🎋", "🎍", "🎎", "🎏", "🎐", "🎑", "🧧", "🎀", "🎁", "🎗️", "🎟️", "🎫"
    ]),

    // Drinks (new)
    ("🍹 Drinks", &[
        "🥤", "🧋", "🧃", "🧉", "🧊", "🥛", "🍼", "🫖", "☕", "🍵", "🍶", "🍾", "🍷", "🍸",
        "🍹", "🍺", "🍻", "🥂", "🥃", "🫗", "🥤", "🧋", "🧃", "🧉", "🧊", "🥢", "🍽️", "🍴",
        "🥄", "🔪", "🫙", "🏺"
    ]),

    // Flags (popular countries)
    ("🏁 Flags", &[
        "🏁", "🚩", "🎌", "🏴", "🏳️", "🏳️‍🌈", "🏳️‍⚧️", "🏴‍☠️", "🇺🇸", "🇬🇧", "🇨🇦", "🇦🇺",
        "🇩🇪", "🇫🇷", "🇮🇹", "🇪🇸", "🇵🇹", "🇧🇷", "🇲🇽", "🇯🇵", "🇰🇷", "🇨🇳", "🇮🇳", "🇷🇺",
        "🇿🇦", "🇳🇬", "🇪🇬", "🇸🇦", "🇦🇪", "🇹🇷", "🇬🇷", "🇳🇱", "🇧🇪", "🇨🇭", "🇦🇹", "🇸🇪",
        "🇳🇴", "🇩🇰", "🇫🇮", "🇵🇱", "🇨🇿", "🇭🇺", "🇷🇴", "🇧🇬", "🇮🇪", "🇦🇷", "🇨🇱", "🇨🇴",
        "🇵🇪", "🇻🇪", "🇺🇾", "🇵🇾", "🇧🇴", "🇪🇨", "🇬🇹", "🇨🇺", "🇩🇴", "🇭🇹", "🇭🇳", "🇳🇮",
        "🇸🇻", "🇨🇷", "🇵🇦", "🇵🇷", "🇯🇲", "🇹🇹", "🇧🇸", "🇧🇧", "🇬🇾", "🇸🇷", "🇫🇴", "🇬🇱",
        "🇮🇸", "🇦🇽", "🇸🇯", "🇱🇮", "🇲🇨", "🇸🇲", "🇻🇦", "🇲🇹", "🇨🇾", "🇬🇪", "🇦🇲", "🇦🇿",
        "🇰🇿", "🇺🇿", "🇹🇲", "🇰🇬", "🇹🇯", "🇦🇫", "🇵🇰", "🇧🇩", "🇱🇰", "🇳🇵", "🇧🇹", "🇲🇻",
        "🇲🇲", "🇹🇭", "🇱🇦", "🇰🇭", "🇻🇳", "🇲🇾", "🇸🇬", "🇧🇳", "🇮🇩", "🇵🇭", "🇹🇱", "🇵🇬",
        "🇦🇺", "🇳🇿", "🇫🇯", "🇳🇨", "🇵🇫", "🇼🇸", "🇹🇴", "🇻🇺", "🇸🇧", "🇰🇮", "🇫🇲", "🇲🇭",
        "🇵🇼", "🇳🇷", "🇹🇻", "🇬🇺", "🇲🇵", "🇦🇸", "🇺🇲"
    ]),
];

#[derive(Clone, PartialEq)]
enum EmojiCategory {
    Recent,          // Recently used emojis
    Custom,          // Custom emojis from user's emoji list
    Set(String),     // Emoji set by identifier
    Standard(usize), // Index into EMOJI_CATEGORIES
}

#[component]
pub fn EmojiPicker(props: EmojiPickerProps) -> Element {
    let mut show_picker = use_signal(|| false);
    let mut selected_category = use_signal(|| EmojiCategory::Recent);
    let mut search_query = use_signal(String::new);
    #[allow(unused_mut)]
    let mut position_below = use_signal(|| false); // Whether to show popup below button
    let button_id = use_signal(|| format!("emoji-picker-{}", uuid::Uuid::new_v4()));
    #[allow(unused_mut)]
    let mut picker_top = use_signal(|| 0.0);
    #[allow(unused_mut)]
    let mut picker_bottom = use_signal(|| 0.0);
    #[allow(unused_mut)]
    let mut picker_left = use_signal(|| 0.0);
    // Track failed image URLs for fallback display
    let mut failed_images: Signal<HashSet<String>> = use_signal(HashSet::new);

    // Read custom emojis, sets, and recent from global state
    let custom_emojis = CUSTOM_EMOJIS.read();
    let emoji_sets = EMOJI_SETS.read();
    let recent_emojis = RECENT_EMOJIS.read();

    // Filter standard emojis based on search (memoized to avoid recomputing on every render)
    let search_lower = use_memo(move || search_query.read().to_lowercase());
    let is_searching = !search_lower.read().is_empty();

    // Memoize search results to avoid re-filtering on every render
    let search_results = use_memo(move || {
        let query = search_lower.read();
        if query.is_empty() {
            return Vec::new();
        }
        EMOJI_CATEGORIES.iter()
            .flat_map(|(_, emojis)| emojis.iter())
            .filter(|e| e.to_lowercase().contains(query.as_str()))
            .take(50) // Limit results for performance
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
    });

    rsx! {
        div {
            class: "relative",

            // Emoji button
            button {
                id: "{button_id}",
                class: if props.icon_only {
                    "p-2 rounded-full hover:bg-accent transition"
                } else {
                    "px-3 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg text-sm font-medium transition"
                },
                title: if props.icon_only { "Add emoji" } else { "" },
                onclick: move |_| {
                    let current = *show_picker.read();
                    show_picker.set(!current);

                    // Calculate position when opening
                    if !current {
                        #[cfg(target_family = "wasm")]
                        {
                            let btn_id = button_id.read().clone();
                            if let Some(window) = web_sys::window() {
                                if let Some(document) = window.document() {
                                    if let Some(element) = document.get_element_by_id(&btn_id) {
                                        let rect = element.get_bounding_client_rect();
                                        let viewport_height = window
                                            .inner_height()
                                            .ok()
                                            .and_then(|h| h.as_f64())
                                            .unwrap_or(800.0);

                                        let button_center_y = rect.top() + (rect.height() / 2.0);
                                        let is_in_top_half = button_center_y < (viewport_height / 2.0);

                                        // Calculate fixed position coordinates
                                        picker_left.set(rect.left());

                                        if is_in_top_half {
                                            // Position below button
                                            picker_top.set(rect.bottom() + 8.0); // 8px margin (mt-2)
                                            position_below.set(true);
                                        } else {
                                            // Position above button
                                            picker_bottom.set(viewport_height - rect.top() + 8.0); // 8px margin (mb-2)
                                            position_below.set(false);
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                if props.icon_only {
                    "😀"
                } else {
                    "😀 Emoji"
                }
            }

            // Emoji picker popover
            if *show_picker.read() {
                div {
                    class: "fixed bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-xl z-[60] w-80",
                    style: if *position_below.read() {
                        format!("top: {}px; left: {}px;", *picker_top.read(), *picker_left.read())
                    } else {
                        format!("bottom: {}px; left: {}px;", *picker_bottom.read(), *picker_left.read())
                    },
                    onclick: move |e| e.stop_propagation(),

                    // Header
                    div {
                        class: "flex items-center justify-between p-3 border-b border-gray-200 dark:border-gray-700",
                        h3 {
                            class: "text-sm font-semibold",
                            "Select Emoji"
                        }
                        button {
                            class: "text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200",
                            onclick: move |_| show_picker.set(false),
                            "✕"
                        }
                    }

                    // Search input
                    div {
                        class: "p-2 border-b border-gray-200 dark:border-gray-700",
                        input {
                            r#type: "text",
                            class: "w-full px-3 py-2 text-sm bg-gray-100 dark:bg-gray-700 border border-gray-200 dark:border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500",
                            placeholder: "Search emojis...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                    }

                    // Category tabs (only show when not searching)
                    if !is_searching {
                        div {
                            class: "flex gap-1 p-2 border-b border-gray-200 dark:border-gray-700 overflow-x-auto",

                            // Recent emojis tab (always first)
                            button {
                                key: "recent",
                                class: if *selected_category.read() == EmojiCategory::Recent {
                                    "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded text-xs font-medium whitespace-nowrap"
                                } else {
                                    "px-2 py-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded text-xs whitespace-nowrap"
                                },
                                onclick: move |_| selected_category.set(EmojiCategory::Recent),
                                "🕐 Recent"
                            }

                            // Custom emojis tab (if user has any)
                            if !custom_emojis.data().read().is_empty() {
                                {
                                    let custom_key = "custom";
                                    rsx! {
                                        button {
                                            key: "{custom_key}",
                                            class: if *selected_category.read() == EmojiCategory::Custom {
                                                "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded text-xs font-medium whitespace-nowrap"
                                            } else {
                                                "px-2 py-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded text-xs whitespace-nowrap"
                                            },
                                            onclick: move |_| selected_category.set(EmojiCategory::Custom),
                                            "⭐ Custom"
                                        }
                                    }
                                }
                            }

                            // Emoji set tabs
                            for set in emoji_sets.data().read().iter() {
                                {
                                    let identifier = set.identifier.clone();
                                    let identifier_for_key = identifier.clone();
                                    let identifier_for_class = identifier.clone();
                                    let set_name = set.name.clone().unwrap_or_else(|| set.identifier.clone());
                                    let display_name = format!("📦 {}", set_name);
                                    rsx! {
                                        button {
                                            key: "set-{identifier_for_key}",
                                            class: if *selected_category.read() == EmojiCategory::Set(identifier_for_class) {
                                                "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded text-xs font-medium whitespace-nowrap"
                                            } else {
                                                "px-2 py-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded text-xs whitespace-nowrap"
                                            },
                                            onclick: move |_| selected_category.set(EmojiCategory::Set(identifier.clone())),
                                            "{display_name}"
                                        }
                                    }
                                }
                            }

                            // Standard emoji categories
                            for (idx, (category_name, _)) in EMOJI_CATEGORIES.iter().enumerate() {
                                button {
                                    key: "std-{idx}",
                                    class: if *selected_category.read() == EmojiCategory::Standard(idx) {
                                        "px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded text-xs font-medium whitespace-nowrap"
                                    } else {
                                        "px-2 py-1 hover:bg-gray-100 dark:hover:bg-gray-700 rounded text-xs whitespace-nowrap"
                                    },
                                    onclick: move |_| selected_category.set(EmojiCategory::Standard(idx)),
                                    "{category_name}"
                                }
                            }
                        }
                    }

                    // Emoji grid
                    div {
                        class: "p-3 max-h-60 overflow-y-auto",

                        // Show search results when searching
                        if is_searching {
                            div {
                                class: "grid grid-cols-7 gap-2",
                                // Render memoized standard emoji search results
                                for (emoji_idx, emoji_str) in search_results.read().iter().enumerate() {
                                    {
                                        let emoji_for_click = emoji_str.clone();
                                        let emoji_display = emoji_str.clone();
                                        rsx! {
                                            button {
                                                key: "search-{emoji_idx}",
                                                class: "text-2xl hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-2 transition",
                                                onclick: move |_| {
                                                    save_recent_emoji(emoji_for_click.clone());
                                                    props.on_emoji_selected.call(emoji_for_click.clone());
                                                    show_picker.set(false);
                                                    search_query.set(String::new());
                                                },
                                                "{emoji_display}"
                                            }
                                        }
                                    }
                                }
                                // Also search custom emojis by shortcode
                                for (emoji_idx, custom_emoji) in custom_emojis.data().read().iter().enumerate() {
                                    if custom_emoji.shortcode.to_lowercase().contains(search_lower.read().as_str()) {
                                        {
                                            let shortcode = custom_emoji.shortcode.clone();
                                            let url = custom_emoji.image_url.clone();
                                            let url_for_click = url.clone();
                                            let url_for_error = url.clone();
                                            let title_text = format!(":{shortcode}:");
                                            let alt_text = format!(":{shortcode}:");
                                            let shortcode_display = format!(":{shortcode}:");
                                            let has_error = failed_images.read().contains(&url);
                                            rsx! {
                                                button {
                                                    key: "search-custom-{emoji_idx}",
                                                    class: "hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-2 transition flex items-center justify-center",
                                                    title: "{title_text}",
                                                    onclick: move |_| {
                                                        save_recent_emoji(url_for_click.clone());
                                                        props.on_emoji_selected.call(format!(" {url_for_click} "));
                                                        show_picker.set(false);
                                                        search_query.set(String::new());
                                                    },
                                                    if has_error {
                                                        span { class: "text-xs text-gray-500 truncate max-w-[4rem]", "{shortcode_display}" }
                                                    } else {
                                                        img {
                                                            src: "{url}",
                                                            alt: "{alt_text}",
                                                            class: "w-8 h-8 object-contain",
                                                            loading: "lazy",
                                                            onerror: move |_| {
                                                                failed_images.write().insert(url_for_error.clone());
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Render based on selected category
                            match selected_category.read().clone() {
                                EmojiCategory::Recent => rsx! {
                                    div {
                                        class: "grid grid-cols-7 gap-2",
                                        for (emoji_idx, emoji) in recent_emojis.iter().enumerate() {
                                            {
                                                let emoji_str = emoji.clone();
                                                let emoji_for_click = emoji_str.clone();
                                                let emoji_for_error = emoji_str.clone();
                                                // Check if it's a URL (custom emoji) or unicode emoji
                                                let is_url = emoji_str.starts_with("http://") || emoji_str.starts_with("https://");
                                                let has_error = is_url && failed_images.read().contains(&emoji_str);
                                                rsx! {
                                                    button {
                                                        key: "recent-{emoji_idx}",
                                                        class: "text-2xl hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-2 transition flex items-center justify-center",
                                                        onclick: move |_| {
                                                            save_recent_emoji(emoji_for_click.clone());
                                                            if is_url {
                                                                props.on_emoji_selected.call(format!(" {} ", emoji_for_click));
                                                            } else {
                                                                props.on_emoji_selected.call(emoji_for_click.clone());
                                                            }
                                                            show_picker.set(false);
                                                        },
                                                        if is_url {
                                                            if has_error {
                                                                span { class: "text-xs text-gray-500", "🖼️" }
                                                            } else {
                                                                img {
                                                                    src: "{emoji_str}",
                                                                    alt: "custom emoji",
                                                                    class: "w-8 h-8 object-contain",
                                                                    loading: "lazy",
                                                                    onerror: move |_| {
                                                                        failed_images.write().insert(emoji_for_error.clone());
                                                                    }
                                                                }
                                                            }
                                                        } else {
                                                            "{emoji_str}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if recent_emojis.is_empty() {
                                            p {
                                                class: "col-span-7 text-center text-gray-500 text-sm py-4",
                                                "No recent emojis yet. Select some emojis to see them here!"
                                            }
                                        }
                                    }
                                },
                                EmojiCategory::Custom => rsx! {
                                    div {
                                        class: "grid grid-cols-5 gap-2",
                                        for (emoji_idx, custom_emoji) in custom_emojis.data().read().iter().enumerate() {
                                            {
                                                let shortcode = custom_emoji.shortcode.clone();
                                                let url = custom_emoji.image_url.clone();
                                                let url_for_click = url.clone();
                                                let url_for_save = url.clone();
                                                let url_for_error = url.clone();
                                                let title_text = format!(":{shortcode}:");
                                                let alt_text = format!(":{shortcode}:");
                                                let shortcode_display = format!(":{shortcode}:");
                                                let has_error = failed_images.read().contains(&url);
                                                rsx! {
                                                    button {
                                                        key: "custom-{emoji_idx}",
                                                        class: "hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-2 transition flex items-center justify-center",
                                                        title: "{title_text}",
                                                        onclick: move |_| {
                                                            save_recent_emoji(url_for_save.clone());
                                                            props.on_emoji_selected.call(format!(" {url_for_click} "));
                                                            show_picker.set(false);
                                                        },
                                                        if has_error {
                                                            span { class: "text-xs text-gray-500 truncate max-w-[4rem]", "{shortcode_display}" }
                                                        } else {
                                                            img {
                                                                src: "{url}",
                                                                alt: "{alt_text}",
                                                                class: "w-8 h-8 object-contain",
                                                                loading: "lazy",
                                                                onerror: move |_| {
                                                                    failed_images.write().insert(url_for_error.clone());
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                EmojiCategory::Set(identifier) => {
                                    let sets_data = emoji_sets.data();
                                    let sets_guard = sets_data.read();
                                    let set = sets_guard.iter().find(|s| s.identifier == identifier);
                                    let set_id = identifier.clone();
                                    rsx! {
                                        div {
                                            class: "grid grid-cols-5 gap-2",
                                            if let Some(set) = set {
                                                for (emoji_idx, custom_emoji) in set.emojis.iter().enumerate() {
                                                    {
                                                        let shortcode = custom_emoji.shortcode.clone();
                                                        let url = custom_emoji.image_url.clone();
                                                        let url_for_click = url.clone();
                                                        let url_for_save = url.clone();
                                                        let url_for_error = url.clone();
                                                        let title_text = format!(":{shortcode}:");
                                                        let alt_text = format!(":{shortcode}:");
                                                        let shortcode_display = format!(":{shortcode}:");
                                                        let has_error = failed_images.read().contains(&url);
                                                        rsx! {
                                                            button {
                                                                key: "set-{set_id}-{emoji_idx}",
                                                                class: "hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-2 transition flex items-center justify-center",
                                                                title: "{title_text}",
                                                                onclick: move |_| {
                                                                    save_recent_emoji(url_for_save.clone());
                                                                    props.on_emoji_selected.call(format!(" {url_for_click} "));
                                                                    show_picker.set(false);
                                                                },
                                                                if has_error {
                                                                    span { class: "text-xs text-gray-500 truncate max-w-[4rem]", "{shortcode_display}" }
                                                                } else {
                                                                    img {
                                                                        src: "{url}",
                                                                        alt: "{alt_text}",
                                                                        class: "w-8 h-8 object-contain",
                                                                        loading: "lazy",
                                                                        onerror: move |_| {
                                                                            failed_images.write().insert(url_for_error.clone());
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            } else {
                                                p {
                                                    class: "col-span-5 text-center text-gray-500 text-sm py-4",
                                                    "Emoji set not found"
                                                }
                                            }
                                        }
                                    }
                                },
                                EmojiCategory::Standard(idx) => rsx! {
                                    div {
                                        class: "grid grid-cols-7 gap-2",
                                        for (emoji_idx, emoji) in EMOJI_CATEGORIES[idx].1.iter().enumerate() {
                                            {
                                                let emoji_str = emoji.to_string();
                                                let emoji_for_click = emoji_str.clone();
                                                rsx! {
                                                    button {
                                                        key: "std-{idx}-{emoji_idx}",
                                                        class: "text-2xl hover:bg-gray-100 dark:hover:bg-gray-700 rounded p-2 transition",
                                                        onclick: move |_| {
                                                            save_recent_emoji(emoji_for_click.clone());
                                                            props.on_emoji_selected.call(emoji_for_click.clone());
                                                            show_picker.set(false);
                                                        },
                                                        "{emoji_str}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}
