export default {
  name: "extract",
  primaryKey: "selector",
  async execute(page, args, ctx) {
    // Pull structured rows out of the page. Each `field` entry is either:
    //   string                   — CSS selector relative to row, take textContent
    //   { selector, attr }       — CSS selector relative to row, take attribute
    //   { selector, text: true } — explicit textContent (default)
    const rowSelector = args.selector;
    const fields = args.fields ?? {};
    const items = await page.$$eval(
      rowSelector,
      (rows, fieldSpec) =>
        rows.map((row) => {
          const out = {};
          for (const [name, spec] of Object.entries(fieldSpec)) {
            const sel = typeof spec === "string" ? spec : spec.selector;
            const attr = typeof spec === "string" ? null : spec.attr ?? null;
            const el = sel ? row.querySelector(sel) : row;
            if (!el) {
              out[name] = null;
              continue;
            }
            out[name] = attr
              ? el.getAttribute(attr)
              : (el.textContent || "").trim();
          }
          return out;
        }),
      fields,
    );
    // Emit as JSON to stdout so wb captures it in step.complete.stdout.
    // Pretty-printed for readability when a runbook surfaces the output.
    console.log(JSON.stringify(items, null, 2));
    if (ctx) ctx.lastResult = items;
    return `${rowSelector} → ${items.length} rows`;
  },
};
