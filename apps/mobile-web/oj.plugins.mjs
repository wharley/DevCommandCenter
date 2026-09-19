import path from "node:path";
import fs from "node:fs";
import { createHash } from "node:crypto";

export default [
  {
    name: "dcc-offline-shell",
    apply: "build",
    closeBundle() {
      const dist = path.resolve(import.meta.dirname, "dist");
      const files = fs
        .readdirSync(dist, { recursive: true })
        .map(String)
        .filter(
          (file) =>
            file !== "sw.js" && fs.statSync(path.join(dist, file)).isFile(),
        )
        .sort();
      const digest = createHash("sha256");
      for (const file of files)
        digest.update(file).update(fs.readFileSync(path.join(dist, file)));
      const template = fs.readFileSync(
        path.resolve(import.meta.dirname, "public/sw.js"),
        "utf8",
      );
      fs.writeFileSync(
        path.join(dist, "sw.js"),
        template
          .replace(
            "/* DCC_PRECACHE */ []",
            JSON.stringify(files.map((file) => `/m/${file}`)),
          )
          .replace("/* DCC_VERSION */", digest.digest("hex").slice(0, 16)),
      );
    },
  },
];
