export interface MessageOptions {
  title?: string;
  message?: string;
  /** The buttons' labels, in the order they're drawn. Defaults to a single OK. */
  buttons?: string[];
  /** The button Enter picks. Defaults to the last one. */
  defaultButton?: number;
  /** The button Escape picks. Defaults to the first one. */
  cancelButton?: number;
}

export interface ConfirmOptions {
  title?: string;
  message?: string;
  okLabel?: string;
  cancelLabel?: string;
}

export interface PromptOptions extends ConfirmOptions {
  value?: string;
  placeholder?: string;
}

export interface SabineDialogApi {
  /** Shows a message over the page and resolves with the button picked. */
  message(options: MessageOptions): Promise<number>;
  /** Resolves with whether OK was picked. */
  confirm(options: ConfirmOptions): Promise<boolean>;
  /** Asks for a line of text, resolving with it, or `null` when cancelled. */
  prompt(options: PromptOptions): Promise<string | null>;
}

/** Messages the window draws over the page, in the app's own pages. */
export declare const dialog: SabineDialogApi;
