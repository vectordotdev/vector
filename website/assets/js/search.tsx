import { autocomplete } from "@algolia/autocomplete-js";
import React, { createElement, Fragment, useEffect, useRef } from "react";
import { createRoot } from "react-dom/client";

declare global {
  interface Window {
    loadExactSearch?: () => Promise<ExactSearchRecord[]>;
    loadPagefind?: () => Promise<any>;
  }
}

type PagefindHit = {
  category: string;
  content: string;
  title: string;
  url: string;
};

type ExactSearchRecord = PagefindHit & {
  aliases: string[];
};

const pagefindCategories = {
  blog: "Blog",
  docs: "Documentation",
  guides: "Guides",
  highlights: "Highlights",
  releases: "Release notes"
};

const configurationCategories = {
  api: "API",
  "global-options": "Global options",
  "pipeline-components": "Pipeline components",
  schema: "Schema",
  secrets: "Secrets",
  sinks: "Sinks",
  sources: "Sources",
  transforms: "Transforms"
};

let pagefindModule: Promise<any> | undefined;
let exactSearchIndex: Promise<ExactSearchRecord[]> | undefined;
const minVrlPrefixLength = 3;

const normalizeSearchTerm = (value: string) =>
  value.trim().toLocaleLowerCase().replace(/[_-]+/g, " ").replace(/\s+/g, " ");

const exactSearchResults = async (query: string): Promise<PagefindHit[]> => {
  if (!window.loadExactSearch) {
    return [];
  }

  exactSearchIndex ??= window.loadExactSearch().catch((error) => {
    exactSearchIndex = undefined;
    throw error;
  });
  const normalizedQuery = normalizeSearchTerm(query);
  const records = await exactSearchIndex;

  if (!normalizedQuery) {
    return [];
  }

  return records
    .filter((record) =>
      record.aliases.some((alias) => {
        const normalizedAlias = normalizeSearchTerm(alias);
        const exactMatch = normalizedAlias === normalizedQuery;
        const vrlPrefixMatch =
          record.category === "VRL function" &&
          normalizedQuery.length >= minVrlPrefixLength &&
          normalizedAlias.startsWith(normalizedQuery);

        return exactMatch || vrlPrefixMatch;
      })
    )
    .map(({ aliases: _, ...record }) => record);
};

const getPagefind = async () => {
  if (!window.loadPagefind) {
    throw new Error("Pagefind is unavailable.");
  }

  pagefindModule ??= window.loadPagefind().catch((error) => {
    pagefindModule = undefined;
    throw error;
  });
  return pagefindModule;
};

const pagefindCategory = (url: string) => {
  const resultUrl = new URL(url, window.location.origin);
  const path = resultUrl.pathname.split("/").filter(Boolean);

  if (path.slice(0, 3).join("/") === "docs/reference/configuration") {
    if (/^#enrichment[-_]tables(?:[._-]|$)/.test(resultUrl.hash)) {
      return "Enrichment tables";
    }

    return configurationCategories[path[3]] ?? "Documentation";
  }

  return pagefindCategories[path[0]] ?? "Website";
};

const pagefindResults = async (query: string): Promise<PagefindHit[]> => {
  const pagefind = await getPagefind();
  const search = await pagefind.debouncedSearch(query);

  if (!search) {
    return [];
  }

  return Promise.all(
    search.results.slice(0, 10).map(async (result) => {
      const data = await result.data();
      const subResult = data.sub_results[0];
      const url = subResult?.url ?? data.url;

      return {
        category: pagefindCategory(url),
        content: subResult?.excerpt ?? data.excerpt ?? "",
        title: subResult?.title ?? data.meta.title ?? data.url,
        url
      };
    })
  );
};

const searchResults = async (query: string): Promise<PagefindHit[]> => {
  const recoverResults = async (results: Promise<PagefindHit[]>, source: string) => {
    try {
      return await results;
    } catch (error) {
      console.warn(`${source} search is unavailable.`, error);
      return [];
    }
  };
  const [exactResults, rankedResults] = await Promise.all([
    recoverResults(exactSearchResults(query), "Exact"),
    recoverResults(pagefindResults(query), "Pagefind")
  ]);
  const exactPages = new Set(exactResults.map((result) => result.url.split("#")[0]));
  const otherResults = rankedResults.filter((result) => !exactPages.has(result.url.split("#")[0]));

  return [...exactResults, ...otherResults].slice(0, 10);
};

const CommandIcon: React.FC = ({ children }) => {
  return (
    <svg width="15" height="15">
      <g fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.2">
        {children}
      </g>
    </svg>
  );
};

const PagefindResult = ({ hit }: { hit: PagefindHit }) => {
  return (
    <a href={hit.url}>
      <div className="border-r border-gray-300 py-4 pl-2 h-full leading-relaxed">{hit.category}</div>
      <div className="p-2 block">
        <div className="text-gray-800 text-md mb-1 font-medium leading-relaxed">{hit.title}</div>
        <p className="text-gray-600 text-sm" dangerouslySetInnerHTML={{ __html: hit.content }} />
      </div>
    </a>
  );
};

const Autocomplete = (props) => {
  const containerRef = useRef(null);
  const panelRootRef = useRef({});

  useEffect(() => {
    if (!containerRef.current) {
      return undefined;
    }

    const search = autocomplete({
      container: containerRef.current,
      renderer: { createElement, Fragment },
      render({ children, state, components }, root) {
        const { preview } = state.context as any;
        if (!panelRootRef.current[root]) {
          panelRootRef.current[root] = createRoot(root);
        }
        panelRootRef.current[root].render(
          <Fragment>
            <div className="aa-Grid">
              <div className="aa-Results aa-Column">{children}</div>
              <div className="aa-Footer border-t">
                <ul className="DocSearch-Commands">
                  <li>
                    <span className="DocSearch-Commands-Key">
                      <CommandIcon>
                        <path d="M12 3.53088v3c0 1-1 2-2 2H4M7 11.53088l-3-3 3-3" />
                      </CommandIcon>
                    </span>
                    <span className="DocSearch-Label">to select</span>
                  </li>
                  <li>
                    <span className="DocSearch-Commands-Key">
                      <CommandIcon>
                        <path d="M7.5 3.5v8M10.5 8.5l-3 3-3-3" />
                      </CommandIcon>
                    </span>
                    <span className="DocSearch-Commands-Key">
                      <CommandIcon>
                        <path d="M7.5 11.5v-8M10.5 6.5l-3-3-3 3" />
                      </CommandIcon>
                    </span>
                    <span className="DocSearch-Label">to navigate</span>
                  </li>
                  <li>
                    <span className="DocSearch-Commands-Key">
                      <CommandIcon>
                        <path d="M13.6167 8.936c-.1065.3583-.6883.962-1.4875.962-.7993 0-1.653-.9165-1.653-2.1258v-.5678c0-1.2548.7896-2.1016 1.653-2.1016.8634 0 1.3601.4778 1.4875 1.0724M9 6c-.1352-.4735-.7506-.9219-1.46-.8972-.7092.0246-1.344.57-1.344 1.2166s.4198.8812 1.3445.9805C8.465 7.3992 8.968 7.9337 9 8.5c.032.5663-.454 1.398-1.4595 1.398C6.6593 9.898 6 9 5.963 8.4851m-1.4748.5368c-.2635.5941-.8099.876-1.5443.876s-1.7073-.6248-1.7073-2.204v-.4603c0-1.0416.721-2.131 1.7073-2.131.9864 0 1.6425 1.031 1.5443 2.2492h-2.956" />
                      </CommandIcon>
                    </span>
                    <span className="DocSearch-Label">to close</span>
                  </li>
                </ul>
              </div>
            </div>
          </Fragment>
        );
      },
      ...props
    });

    return () => {
      search.destroy();
    };
  }, [props]);

  return <div ref={containerRef} />;
};

const Search = () => {
  return (
    <Autocomplete
      aria-label="Search query results"
      openOnFocus={false}
      detachedMediaQuery=""
      defaultActiveItemId={0}
      placeholder="Search"
      getSources={({ query }) => [
        {
          sourceId: "queryResults",
          async getItems() {
            return searchResults(query);
          },
          getItemUrl({ item }) {
            return item.url;
          },
          templates: {
            item({ item }) {
              return <PagefindResult hit={item} />;
            },
            noResults() {
              return "No results found.";
            }
          }
        }
      ]}
    />
  );
};

const searchRoot = createRoot(document.getElementById("site-search"));
searchRoot.render(<Search />);
