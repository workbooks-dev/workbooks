export default {
  name: "assert",
  primaryKey: "selector",
  async execute(page, args) {
    const sel = args.selector;
    const handle = await page.$(sel);
    if (!handle) throw new Error(`assert: selector not found: ${sel}`);
    if (args.text_contains) {
      const txt = (await handle.textContent()) ?? "";
      if (!txt.includes(args.text_contains)) {
        throw new Error(
          `assert: text "${args.text_contains}" not in ${sel} (got "${txt.slice(0, 80)}")`,
        );
      }
    }
    if (args.url_contains && !page.url().includes(args.url_contains)) {
      throw new Error(
        `assert: url does not contain "${args.url_contains}" (got ${page.url()})`,
      );
    }
    return `${sel}`;
  },
};
