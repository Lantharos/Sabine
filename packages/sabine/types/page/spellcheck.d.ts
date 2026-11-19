export interface SpellcheckSettings {
  enabled: boolean;
  /** Language tags such as `en-US`, the first suggesting corrections first. */
  languages: string[];
}

/**
 * Spellchecking in the app's pages. Changes last in the app's profile, until
 * the app sets other settings in `Sabine.toml` or Rust.
 */
export interface SabineSpellcheckApi {
  get(): Promise<SpellcheckSettings>;
  /** Changes what it is given, and resolves with the settings as they are then. */
  set(settings: Partial<SpellcheckSettings>): Promise<SpellcheckSettings>;
  /** Adds a word to the custom dictionary, which every window of the app shares. */
  addWord(word: string): Promise<void>;
  /** Replaces the misspelled word the last context menu was asked for over. */
  replace(word: string): Promise<void>;
}

export declare const spellcheck: SabineSpellcheckApi;
