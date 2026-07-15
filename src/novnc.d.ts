// @novnc/novnc ships no TypeScript types; declare the bit we use.
// The package's only export is the root, which is core/rfb.js.
declare module "@novnc/novnc" {
  export default class RFB extends EventTarget {
    constructor(
      target: HTMLElement,
      url: string,
      options?: { credentials?: { password?: string }; wsProtocols?: string[] },
    );
    scaleViewport: boolean;
    resizeSession: boolean;
    disconnect(): void;
  }
}
