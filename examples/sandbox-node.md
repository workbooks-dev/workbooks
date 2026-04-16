---
title: Node Sandbox
requires:
  sandbox: node
  apt: [jq]
  node: [chalk, lodash]
---

# Node Sandbox

Runs in an isolated container with Node, globally installed npm packages, and system tools.

## Verify system deps

```bash
echo "node: $(node --version)"
echo "npm: $(npm --version)"
echo "jq: $(jq --version)"
```

## Verify npm packages

```node
const chalk = require('chalk');
const _ = require('lodash');

console.log(chalk.green('chalk') + ' and ' + chalk.blue('lodash') + ' are installed globally!');
console.log('lodash version:', _.VERSION);
```

## Use lodash in node

```node
const data = [
    { name: 'Alice', dept: 'eng' },
    { name: 'Bob', dept: 'eng' },
    { name: 'Carol', dept: 'sales' },
    { name: 'Dave', dept: 'sales' },
    { name: 'Eve', dept: 'eng' },
];

const grouped = _.groupBy(data, 'dept');
for (const [dept, members] of Object.entries(grouped)) {
    const names = _.map(members, 'name').join(', ');
    console.log(chalk.cyan(dept + ':'), names);
}
```

## Pipe through jq

```bash
echo '{"sandbox": "node", "status": "working", "tools": ["jq", "chalk", "lodash"]}' | jq .
```
