import { redact } from "../lib/util.js";

export default {
  name: "fill",
  primaryKey: "selector",
  async execute(page, args) {
    // Don't echo the value into the summary — could be a credential.
    await page.fill(args.selector, String(args.value ?? ""), {
      timeout: args.timeout ?? 10_000,
    });
    return `${args.selector} = «${redact(args.value)}»`;
  },
};
