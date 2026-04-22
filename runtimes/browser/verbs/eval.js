export default {
  name: "eval",
  primaryKey: "script",
  async execute(page, args, ctx) {
    // Run arbitrary JS in the page; result is JSON-serialized to stdout.
    const result = await page.evaluate(args.script);
    console.log(JSON.stringify(result, null, 2));
    if (ctx) ctx.lastResult = result;
    return `script ran`;
  },
};
