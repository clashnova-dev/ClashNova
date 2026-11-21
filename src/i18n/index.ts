// Explicit extensions keep this module loadable by the Node test runner as well as by Vite.
import zhCN from "./zh-CN.ts";
import en from "./en.ts";
import ja from "./ja.ts";
import ko from "./ko.ts";
import de from "./de.ts";
import fr from "./fr.ts";
import ru from "./ru.ts";

export type Locale = "zh-CN" | "en" | "ja" | "ko" | "de" | "fr" | "ru";
export type MessageKey = keyof typeof zhCN;
/** A key together with the values to fill in. A value may itself be a message. */
export type MessageObject = { key: MessageKey; values?: MessageValues };
export type MessageValues = Record<string, string | number | MessageObject>;
/**
 * Anything that can be shown to the user: app copy (a key or a prepared message),
 * or verbatim text such as a node name or an error reported by a provider.
 */
export type Text = MessageKey | MessageObject | string;

const dictionaries: Record<Locale, Record<MessageKey, string>> = { "zh-CN": zhCN, en, ja, ko, de, fr, ru };
const own = (object: object, name: string) => Object.prototype.hasOwnProperty.call(object, name);

export const messageKeys = Object.keys(zhCN) as MessageKey[];
export const isMessageKey = (text: string): text is MessageKey => own(zhCN, text);
/** Prepares a message to be stored and rendered later, so it follows the language in use when it is shown. */
export const msg = (key: MessageKey, values?: MessageValues): MessageObject => ({ key, values });

/**
 * The wire form of a message, used wherever text crosses a boundary as a plain string
 * (errors and status from the native side, lines in the log file):
 * a bare key, or `{"key":"…","values":{…}}` when there are values. Verbatim text is sent as is.
 */
export function encode(text: Text): string {
  if (typeof text === "string") return text;
  return text.values && Object.keys(text.values).length ? JSON.stringify({ key: text.key, values: text.values }) : text.key;
}
function decode(text: string): MessageObject | undefined {
  if (!text.startsWith('{"key":"')) return undefined;
  try {
    const parsed = JSON.parse(text) as { key?: unknown; values?: unknown };
    if (typeof parsed.key !== "string" || !isMessageKey(parsed.key)) return undefined;
    return { key: parsed.key, values: parsed.values && typeof parsed.values === "object" ? parsed.values as MessageValues : undefined };
  } catch { return undefined; }
}

function render(locale: Locale, text: Text | null | undefined, values?: MessageValues): string {
  if (text == null || text === "") return "";
  if (typeof text === "object") return render(locale, text.key, { ...text.values, ...values });
  if (!isMessageKey(text)) { const message = decode(text); return message ? render(locale, message) : text; }
  return dictionaries[locale][text].replace(/\{(\w+)\}/g, (token, name: string) => {
    const value = values?.[name];
    // A value can be a message itself, e.g. a native error key inside "Test failed: {error}".
    return value === undefined ? token : typeof value === "number" ? String(value) : render(locale, value);
  });
}

/**
 * `t` renders a key, which the compiler checks against the dictionary.
 * `tx` renders anything that may reach the screen, including verbatim text, which it leaves as is.
 */
export function translator(locale: () => Locale) {
  return {
    t: (key: MessageKey, values?: MessageValues) => render(locale(), key, values),
    tx: (text: Text | null | undefined, values?: MessageValues) => render(locale(), text, values),
  };
}
