export default {
  name: "eval",
  primaryKey: "script",
  async execute(page, args, ctx) {
    // Wrap the script in an async IIFE so authors can write function-body
    // style: top-level `return X` works, top-level `await X` works, and
    // multi-statement scripts read like the `(async () => { ... })()`
    // pattern people already write into runbooks. Trade-off: bare-expression
    // scripts (`script: "1 + 1"`) no longer return their value — authors
    // must say `return 1 + 1` explicitly. That migration is intentional —
    // multi-line scripts are the common case and "must add `return`" is a
    // clearer rule than "single expressions vs. statement bodies behave
    // differently."
    const wrapped = `(async () => { ${args.script} })()`;
    const result = await page.evaluate(wrapped);
    console.log(JSON.stringify(result, null, 2));
    if (ctx) ctx.lastResult = result;
    return `script ran`;
  },
};
