export default {
  name: "click",
  primaryKey: "selector",
  async execute(page, args) {
    await page.click(args.selector, { timeout: args.timeout ?? 10_000 });
    return `${args.selector}`;
  },
};
