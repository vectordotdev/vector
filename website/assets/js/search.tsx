import { autocomplete } from '@algolia/autocomplete-js'
import Typesense from 'typesense'
import React, { createElement, Fragment, useEffect, useRef } from 'react'
import ReactDOM, { render } from 'react-dom'

// // Algolia search
// const appId = process.env.ALGOLIA_APP_ID
const apiKey = process.env.TYPESENSE_PUBLIC_API_KEY
const indexName = process.env.TYPESENSE_INDEX
const host = process.env.TYPESENSE_HOST
// const searchClient = algoliasearch(appId, apiKey)

let searchClient = new Typesense.Client({
  apiKey: apiKey,
  nodes: [
    {
      host: `${host}.a1.typesense.net`,
      port: '443',
      protocol: 'https',
    },
  ],
  connectionTimeoutSeconds: 2,
})


const CommandIcon: React.FC = ({ children }) => {
  return (
    <svg width="15" height="15">
      <g
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.2"
      >
        {children}
      </g>
    </svg>
  )
}

const Chevron: React.FC = () => {
  return (
    <svg
      className="h-3 w-3 inline"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
    >
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M9 5l7 7-7 7"
      />
    </svg>
  )
}


const Result = ({ hit, components, category }) => {
  const hierarchy = hit.document.hierarchy.concat(hit.document.title)
  const isRootPage = hierarchy.length < 1

  return (
    <a href={hit.document.itemUrl}>
      <div className="border-r border-gray-300 py-4 pl-2 h-full leading-relaxed">
        {category}
      </div>
      <div className="p-2 block">
        <div className="text-gray-800 text-md mb-1 font-medium leading-relaxed ">
          {!isRootPage &&
            hierarchy.map((t, i) => (
              <span key={`${hit.document.itemUrl}-${t}`}>
                <span className="w-2 h-2 inline" key={`${t.itemUrl}`}>
                  {t}
                </span>
                {i < hierarchy.length - 1 && (
                  <span className="inline ml-1 mr-1">
                    <Chevron />
                  </span>
                )}
              </span>
            ))}
          {isRootPage && <components.Highlight hit={hit} attribute="title" />}
        </div>
        <p className="text-gray-600 text-sm">
          {hit.content && (
            <span dangerouslySetInnerHTML={{__html: hit.content}} />
          )}
          {!hit.content && (
            <span style={{ wordBreak: 'break-word' }}>{hit.document.itemUrl}</span>
          )}
        </p>
      </div>
    </a>
  )
}

const Autocomplete = (props) => {
  const containerRef = useRef(null)

  useEffect(() => {
    if (!containerRef.current) {
      return undefined
    }

    const search = autocomplete({
      container: containerRef.current,
      renderer: { createElement, Fragment },
      render({ children, state, components }, root) {
        const { preview } = state.context as any
        render(
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
          </Fragment>,
          root,
        )
      },
      ...props,
    })

    return () => {
      search.destroy()
    }
  }, [props])

  return <div ref={containerRef} />
}

const Search = () => {
  return (
    <Autocomplete
      aria-label="Search query results"
      openOnFocus={false}
      detachedMediaQuery=""
      defaultActiveItemId={0}
      getSources={({ query }) => [
        {
          sourceId: 'queryResults',
          async getItems() {
            const results = (query) => searchClient.collections('vector_docs').documents().search({
              q: query,
              preset: 'vector_docs_search',
              exhaustive_search: true,
              highlight_fields: 'content',
              highlight_full_fields: 'content'

            }).then((result) => {
              // order the hits by page group
              // const hits = result.hits.sort((a, b) => (a.document.pageTitle < b.document.pageTitle ? -1 : 1))

              // add page as category if there are duplicates
              const hitsWithCategory = result.hits.map((h, i) => {
                const prev = result.hits[i - 1] as any
                const title = h.document.pageTitle

                // if no previous hit is in this category
                if (!prev) {
                  return { ...h, category: title }
                }

                // skip if there is already one in this category
                if (prev && prev.document.pageTitle === title) {
                  return h
                }

                // add category if needed
                if (prev && prev.document.pageTitle !== title) {
                  return { ...h, category: title }
                }

                return h
              })
              return hitsWithCategory
            })
            return await results(query)
          },
          getItemUrl({ item }) {
            return item.document.itemUrl
          },
          templates: {
            item({ item, components }) {
              const highlight = item.highlights.length && item.highlights.find(h => h.field === 'content' || {}).value || item.document['content']
              item['content'] = highlight
              return <Result hit={item} components={components} category={item.category} />
            },
            noResults() {
              return 'No results found.';
            },
          },
        }
      ]}
    />
  )
}


ReactDOM.render(<Search />, document.getElementById('site-search'))
