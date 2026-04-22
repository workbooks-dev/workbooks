export default {
  name: "press",
  primaryKey: "key",
  async execute(page, args) {
    const target = args.selector ?? "body";
    await page.press(target, args.key, { timeout: args.timeout ?? 5_000 });
    return `${target} ⌨ ${args.key}`;
  },
};
