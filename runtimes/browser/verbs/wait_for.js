export default {
  name: "wait_for",
  primaryKey: "selector",
  async execute(page, args) {
    const selector = args.selector;
    const state = args.state ?? "visible";
    await page.waitForSelector(selector, {
      state,
      timeout: args.timeout ?? 15_000,
    });
    return `${selector} (${state})`;
  },
};
