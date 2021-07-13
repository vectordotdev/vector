import React, { createElement, Fragment, useEffect, useRef } from 'react'
import ReactDOM, { render } from 'react-dom'
import algoliasearch from 'algoliasearch/lite'
import { autocomplete, getAlgoliaResults } from '@algolia/autocomplete-js'

// Algolia search
const appId = process.env.ALGOLIA_APP_ID
const apiKey = process.env.ALGOLIA_PUBLIC_API_KEY
const indexName = process.env.ALGOLIA_INDEX_NAME
const searchClient = algoliasearch(appId, apiKey)

const Result = ({ hit, components }) => {
  return (
    <a href={hit.itemUrl}>
      <div className="pl-2">
        <p className="text-gray-800 text-md mb-1 font-medium leading-relaxed">
          <components.Highlight hit={hit} attribute="title" />
        </p>
        <p className="text-gray-600 text-sm">
          <components.Snippet hit={hit} attribute="content" />
        </p>
      </div>
    </a>
  )
}

export function Autocomplete(props: any) {
  const containerRef = useRef(null)

  useEffect(() => {
    if (!containerRef.current) {
      return undefined
    }

    const search = autocomplete({
      container: containerRef.current,
      renderer: { createElement, Fragment },
      render({ children, state, components }, root) {
        const { preview } = state.context
        render(
          <Fragment>
            <div className="aa-Grid">
              <div className="aa-Results aa-Column border-gray-200 border-r p-2">
                {children}
              </div>
              {false && (
                <div className="aa-Preview aa-Column p-4">
                  <div className="aa-PreviewTitle">
                    <components.Highlight hit={preview} attribute={['title']} />
                  </div>
                  <div className="aa-PreviewDescription">
                    <components.Highlight
                      hit={preview}
                      attribute={['summary']}
                    />
                  </div>
                </div>
              )}
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
    <div id="doc-search" style={{ width: 300 }}>
      <Autocomplete
        openOnFocus={false}
        defaultActiveItemId={0}
        placeholder="Search documentation"
        debug
        getSources={({ query }) => [
          {
            sourceId: 'queryResults',
            getItems() {
              return getAlgoliaResults({
                searchClient,
                queries: [
                  {
                    indexName,
                    query,
                  },
                ],
              })
            },
            getItemUrl({ item }) {
              return item.itemUrl
            },
            onActive({ item, setContext }) {
              setContext({ preview: item })
            },
            templates: {
              item({ item, components }) {
                return <Result hit={item} components={components} />
              },
            },
          },
          {
            sourceId: 'suggestions',
            getItems({ query }) {
              return getAlgoliaResults({
                searchClient,
                queries: [
                  {
                    indexName,
                    query,
                    params: {
                      hitsPerPage: 4,
                    },
                  },
                ],
              })
            },
            onSelect({ item, setQuery, setIsOpen, refresh }) {
              setQuery(`${item.query} `)
              setIsOpen(true)
              refresh()
            },
            templates: {
              header({ items, Fragment }) {
                if (items.length === 0) {
                  return null
                }

                return (
                  <Fragment>
                    <span className="aa-SourceHeaderTitle">
                      Can't find what you're looking for?
                    </span>
                    <div className="aa-SourceHeaderLine" />
                  </Fragment>
                )
              },
              item({ item, components }) {
                return (
                  <div className="aa-QuerySuggestion">
                    <components.ReverseHighlight hit={item} attribute="title" />
                  </div>
                )
              },
            },
          },
        ]}
      />
    </div>
  )
}

ReactDOM.render(<Search />, document.getElementById('site-search'))
