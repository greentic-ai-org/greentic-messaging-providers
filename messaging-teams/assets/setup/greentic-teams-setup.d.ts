export interface GreenticTeamsSetupTranslations {
  [key: string]: string | GreenticTeamsSetupTranslations;
}

export interface GreenticTeamsSetupDeviceLogin {
  url: string;
  userCode?: string;
  message?: string;
  interval?: number;
}

export declare class GreenticTeamsSetup extends HTMLElement {
  static translations: Record<string, GreenticTeamsSetupTranslations>;

  translations: GreenticTeamsSetupTranslations | null;

  readonly apiBase: string;
  readonly locale: string;
  readonly pollInterval: number;
  readonly autoPoll: boolean;
  readonly actionTimeout: number;
  readonly providerId: string;

  /**
   * Resolve a configured endpoint path. Path attributes use tester-compatible
   * defaults and may include a `{kind}` placeholder for OAuth routes.
   */
  _endpoint(name: string, params?: Record<string, string>): string;

  refresh(): Promise<void>;
  runCurrentAction(): Promise<unknown>;
  retryCurrentAction(): Promise<unknown>;
  runNextStep(): Promise<unknown>;
  saveConfiguration(): Promise<unknown>;
  publishTeamsApp(): Promise<unknown>;
  installTeamsApp(): Promise<unknown>;
}

declare global {
  interface HTMLElementTagNameMap {
    "greentic-teams-setup": GreenticTeamsSetup;
  }
}
