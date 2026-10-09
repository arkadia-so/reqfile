const fs = require("fs");

export const read = (path: string) => fs.readFileSync(path, "utf8");
