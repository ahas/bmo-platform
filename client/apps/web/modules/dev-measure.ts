import { defineEventHandler, setResponseHeader } from "h3";
import { defineNuxtModule } from "nuxt/kit";

import { measureSource } from "../../../tests/browser/measure-source";

// Serves the screen-measuring module to the page, in development only.
//
// A check drives the console through a browser whose only way into the page is a script it is
// handed, so the measuring module has to be reachable by an address the page can import. It is the
// checking harness rather than part of the console, so the built console never carries it and it
// stands only where a check ever runs: the development server a worker keeps.
export default defineNuxtModule({
  meta: { name: "bmo-dev-measure" },
  setup(_options, nuxt) {
    if (!nuxt.options.dev) {
      return;
    }

    nuxt.hook("nitro:config", (config) => {
      config.devHandlers = [...(config.devHandlers ?? []), {
        route: `${nuxt.options.app.baseURL.replace(/\/$/, "")}/measure.js`,
        handler: defineEventHandler((event) => {
          setResponseHeader(event, "content-type", "text/javascript; charset=utf-8");

          return measureSource();
        })
      }];
    });
  }
});
