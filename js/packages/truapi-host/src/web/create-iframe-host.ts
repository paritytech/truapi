/**
 * Options for `createIframeHost`.
 */
export interface IframeHostOptions {
  /** URL of the product iframe. */
  iframeUrl: string;
  /** Container element the iframe is appended to. */
  container: HTMLElement;
  /**
   * Called with one end of the MessageChannel once the iframe has loaded.
   * Hosts typically pipe this into a `WireProvider` (e.g. via
   * `createMessagePortProvider` from `@parity/truapi`).
   */
  onPort: (port: MessagePort) => void;
  /**
   * Optional explicit allow-list origin. Defaults to the origin of
   * `iframeUrl`. Throws if it disagrees with the iframe URL's origin.
   */
  allowedOrigin?: string;
  /** Optional iframe Permissions Policy allow attribute. */
  allow?: string;
  /** Override the default iframe sandbox attribute. */
  sandbox?: string;
}

/**
 * Handle returned by `createIframeHost`.
 */
export interface IframeHost {
  iframe: HTMLIFrameElement;
  dispose: () => void;
}

const DEFAULT_IFRAME_SANDBOX = "allow-forms allow-same-origin allow-scripts";
type CredentiallessIframe = HTMLIFrameElement & { credentialless?: boolean };

function resolveAllowedOrigin(
  iframeUrl: string,
  allowedOrigin?: string,
): string {
  const targetUrl = new URL(iframeUrl, window.location.href);
  if (targetUrl.protocol !== "http:" && targetUrl.protocol !== "https:") {
    throw new Error(
      `Iframe host only allows http(s) product URLs, received ${targetUrl.protocol}`,
    );
  }

  if (!allowedOrigin) {
    return targetUrl.origin;
  }

  const normalizedOrigin = new URL(allowedOrigin).origin;
  if (normalizedOrigin !== targetUrl.origin) {
    throw new Error(
      `Iframe host origin policy mismatch, expected ${normalizedOrigin}, got ${targetUrl.origin}`,
    );
  }

  return normalizedOrigin;
}

/**
 * Embed a product iframe and transfer a `MessagePort` into it. The host
 * keeps the other end and passes it to a `WireProvider` (typically via
 * `createMessagePortProvider`). All product traffic flows over the
 * MessageChannel.
 */
export function createIframeHost(options: IframeHostOptions): IframeHost {
  const {
    iframeUrl,
    container,
    onPort,
    allowedOrigin,
    allow,
    sandbox = DEFAULT_IFRAME_SANDBOX,
  } = options;

  const channel = new MessageChannel();
  const hostPort = channel.port1;
  const productPort = channel.port2;
  const targetOrigin = resolveAllowedOrigin(iframeUrl, allowedOrigin);

  // Hand the host-side port to the caller immediately so it can wire up
  // a provider before the iframe finishes loading. Queued postMessage
  // calls are delivered once the channel is started by the provider.
  onPort(hostPort);

  const iframe = document.createElement("iframe");
  iframe.style.width = "100%";
  iframe.style.height = "100%";
  iframe.style.border = "none";
  // COEP hosts need credentialless product iframes when the product origin
  // does not serve matching embedder headers.
  const credentiallessIframe = iframe as CredentiallessIframe;
  credentiallessIframe.credentialless = true;
  if (allow !== undefined) {
    iframe.allow = allow;
  }
  iframe.setAttribute("sandbox", sandbox);
  iframe.referrerPolicy = "no-referrer";
  iframe.src = iframeUrl;
  const initTargetOrigin = credentiallessIframe.credentialless
    ? "*"
    : targetOrigin;

  let initSent = false;
  const sendInit = (): void => {
    if (initSent) return;
    const contentWindow = iframe.contentWindow;
    if (!contentWindow) return;
    initSent = true;
    contentWindow.postMessage({ type: "truapi-init" }, initTargetOrigin, [
      productPort,
    ]);
  };

  const onWindowMessage = (event: MessageEvent): void => {
    if (event.source !== iframe.contentWindow) return;
    if (event.origin !== targetOrigin && event.origin !== "null") return;
    if (event.data?.type === "truapi-ready") {
      sendInit();
    }
  };
  window.addEventListener("message", onWindowMessage);

  container.appendChild(iframe);

  return {
    iframe,
    dispose() {
      window.removeEventListener("message", onWindowMessage);
      try {
        hostPort.close();
      } catch {
        // already closed
      }
      try {
        productPort.close();
      } catch {
        // already closed
      }
      iframe.remove();
    },
  };
}

export type { WireProvider } from "@parity/truapi";
