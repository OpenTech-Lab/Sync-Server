/**
 * Global JSX type extension for the ALTCHA web component.
 *
 * The `altcha` npm package registers `<altcha-widget>` as a custom element but
 * does not ship React JSX type declarations.  We add them here so TypeScript
 * and the React JSX transform accept the element without @ts-ignore.
 */

import type { RefObject } from "react";

declare module "react" {
  namespace JSX {
    interface IntrinsicElements {
      "altcha-widget": React.DetailedHTMLProps<
        React.HTMLAttributes<HTMLElement> & {
          challengeurl?: string;
          challengejson?: string;
          hidefooter?: boolean | "" | undefined;
          hidelogo?: boolean | "" | undefined;
          auto?: "off" | "onfocus" | "onload" | "onsubmit";
          delay?: number;
          expire?: number;
          language?: string;
          name?: string;
          ref?: RefObject<HTMLElement | null> | null;
        },
        HTMLElement
      >;
    }
  }
}
