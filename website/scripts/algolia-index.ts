import chalk from "chalk";
import cheerio from "cheerio";
import { Element } from "domhandler";
import dotEnv from "dotenv-defaults";
import fs from "fs";
import glob from "glob-promise";
import path from "path";

dotEnv.config();

// Types
type Payload = {
  level: number;
  domId?: string;
  tagName: string;
  content: string;
}[];

type AlgoliaRecord = {
  objectID: string;
  pageTitle: string;
  pageUrl: string;
  itemUrl: string;
  level: number;
  title: string;
  hierarchy: string[];
  tags: string[];
  ranking: number;
  section: string;
  content: string;
};

type Section = {
  name: string;
  path: string;
  displayPath: string;
  ranking: number;
};

// Constants
const DEBUG = process.env.DEBUG === "true" || false;
const targetFile = "./public/search.json";
const publicPath = path.resolve(__dirname, "..", "public");
const tagHierarchy = {
  h1: 6,
  h2: 5,
  h3: 4,
  h4: 3,
  h5: 2,
  h6: 1,
  li: 1,
  p: 1,
};

function getPageUrl(file: string) {
  const filePath = file.split("public/")[1].split(path.sep).slice(0, -1);
  return `/${filePath.join("/")}`;
}

function getItemUrl(file: string, { level, domId }: Payload[0]) {
  const fileUrl = getPageUrl(file);

  if (level > 1 && level < 6 && !domId) {
    console.log(chalk.yellow(`Missing domId for level ${level}`));
    console.log(chalk.yellow(`File ${file}`));
  }

  return level > 1 && level < 6 && domId ? `${fileUrl}#${domId}` : fileUrl;
}

async function indexHTMLFiles(
  records: AlgoliaRecord[],
  section: string,
  files: string[],
  ranking: number
): Promise<void> {
  const usedIds = {};
  const algoliaRecords: AlgoliaRecord[] = [];

  for (const file of files) {
    const html = fs.readFileSync(file, "utf-8");
    const $ = cheerio.load(html);
    const containers = $("#page-content");
    const pageTitle = $("meta[name='algolia:title']").attr("content") || "";
    const pageTagsString = $("meta[name='keywords']").attr('content') || "";
    const pageTags: string[] = (pageTagsString === "") ? [] : pageTagsString.split(",");

    // @ts-ignore
    $(".algolia-no-index").each((_, d) => $(d).remove());
    // @ts-ignore
    $(".highlight").each((_, d) => $(d).remove());
    const payload: Payload = [];
    const traverse = (node?: Element) => {
      if (!node) {
        return;
      }

      const level = tagHierarchy[node.tagName];

      if (level) {
        payload.push({
          level,
          domId: $(node).attr("id"),
          tagName: node.tagName,
          content: $(node)
            .text()
            .replace(/[\n\t]/g, " "),
        });
      }

      $(node)
        .children()
        .map((_, d) => traverse(d));
    };

    for (let i = 0; i < containers.length; i++) {
      traverse(containers.get(i) as Element);
    }

    let activeRecord: AlgoliaRecord | null = null;

    for (const item of payload) {
      const pageUrl = getPageUrl(file);
      const itemUrl = getItemUrl(file, item);

      if (!activeRecord) {
        activeRecord = {
          objectID: itemUrl,
          pageTitle,
          pageUrl,
          itemUrl,
          level: item.level,
          title: item.content.trim(),
          section,
          ranking,
          hierarchy: [],
          tags: pageTags,
          content: "",
        };
      } else if (item.level === 1) { // h1 logic
        activeRecord.content += item.content;
      } else if (item.level < activeRecord.level) {
        algoliaRecords.push({ ...activeRecord });

        activeRecord = {
          objectID: itemUrl,
          pageTitle,
          pageUrl,
          itemUrl,
          level: item.level,
          title: item.content.trim(),
          section,
          ranking,
          hierarchy: [...activeRecord.hierarchy, activeRecord.title.trim()],
          tags: pageTags,
          content: "",
        };
      } else { // h2-h6 logic
        algoliaRecords.push({ ...activeRecord });

        const hierarchySize = activeRecord.hierarchy.length;
        const levelDiff = item.level - activeRecord.level;
        const lastIndex = hierarchySize - levelDiff;

        activeRecord = {
          objectID: itemUrl,
          pageTitle,
          pageUrl,
          itemUrl,
          level: item.level,
          title: item.content.trim(),
          section,
          ranking,
          hierarchy: [...activeRecord.hierarchy.slice(0, lastIndex)],
          tags: pageTags,
          content: "",
        };
      }

      if (activeRecord) {
        algoliaRecords.push({ ...activeRecord });
      }

      for (const rec of algoliaRecords) {
        // The objectID is the url of the section of the page that the record covers.
        // If you have a duplicate here somehow two records point to the same thing.
        if (DEBUG && usedIds[rec.objectID]) {
          console.log(chalk.yellow(`Duplicate ID for ${rec.objectID}`));
          console.log(JSON.stringify(rec, null, 2));
        }

        usedIds[rec.objectID] = true;

        // The h2 -> h5 should have a set of tags that are the "path" within the file.
        if (DEBUG && rec.level > 1 && rec.level < 6 && rec.hierarchy.length == 0) {
          console.log(chalk.yellow("Found h2 -> h5 with no tags."));
          console.log(JSON.stringify(rec, null, 2));
        }
      }
    }
  }

  console.log(
    chalk.green(`Success. Updated records for ${files.length} file(s).`)
  );

  records.push(...algoliaRecords);
}

async function buildIndex() {
  var allRecords: AlgoliaRecord[] = [];

  console.log(`Building Vector search index`);

  const sections: Section[] = [
    {
      name: "Docs",
      path: `${publicPath}/docs/about/**/**.html`,
      displayPath: "docs/about",
      ranking: 50,
    },
    {
      name: "Docs",
      path: `${publicPath}/docs/administration/**/**.html`,
      displayPath: "docs/administration",
      ranking: 50,
    },
    {
      name: "Docs",
      path: `${publicPath}/docs/reference/**/**.html`,
      displayPath: "docs/reference",
      ranking: 50,
    },
    {
      name: "Docs",
      path: `${publicPath}/docs/setup/**/**.html`,
      displayPath: "docs/setup",
      ranking: 50,
    },
    {
      name: "Advanced guides",
      path: `${publicPath}/guides/advanced/**/**.html`,
      displayPath: "guides/advanced",
      ranking: 40,
    },
    {
      name: "Level up guides",
      path: `${publicPath}/guides/level-up/**/**.html`,
      displayPath: "guides/level-up",
      ranking: 40,
    }
  ];

  // Recurse through each section and push the resulting records to `allRecords`
  for (const section of sections) {
    let files = await glob(section.path);
    console.log(chalk.blue(`Indexing ${section.displayPath}...`));
    indexHTMLFiles(allRecords, section.name, files, section.ranking);
  }

  console.log(chalk.green(`Success. ${allRecords.length} records have been successfully indexed.`));
  console.log(chalk.blue(`Writing final index JSON to ${targetFile}...`));

  const recordsJson: string = JSON.stringify(allRecords);

  fs.writeFile(targetFile, recordsJson, () => {
    console.log(chalk.green(`Success. Wrote final index JSON to ${targetFile}.`));
  });
}

buildIndex().catch((err) => {
  console.trace(chalk.yellow(err));
});
