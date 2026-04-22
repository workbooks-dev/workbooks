export default {
  name: "goto",
  primaryKey: "url",
  async execute(page, args) {
    const url = args.url ?? "";
    const waitUntil = args.wait_until ?? "domcontentloaded";
    await page.goto(url, { waitUntil, timeout: args.timeout ?? 30_000 });
    return `→ ${page.url()}`;
  },
};
