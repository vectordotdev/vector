import React from 'react';

import pluralize from 'pluralize';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function downcaseFirst(text) {
  return text.charAt(0).toLowerCase() + text.slice(1);
}

function ServiceDiagram({sourceName, sinkName}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {sources: sourcesMap, sinks: sinksMap}} = siteConfig.customFields;
  const source = sourcesMap[sourceName];
  const sink = sinksMap[sinkName];

  let receiveTitle = source ?
    `Vector ${pluralize(source.function_category)} data from ${source.noun}` :
    'Vector receives data'

  let receiveDescription = source ?
    `Vector will ${downcaseFirst(source.features[0])}` :
    `Vector receives data from another upstream Vector instance.`

  let forwardTitle = sink ?
    `Vector ${pluralize(sink.function_category)} data to ${sink.noun}` :
    'Vector fans-out data'

  let forwardDescription = sink ?
    `Vector will ${downcaseFirst(sink.features[0])}` :
    `Vector receives data from another upstream Vector instance.`

  return (
    <svg width="850px" height="375px" viewBox="0 0 850 375" version="1.1" xmlns="http://www.w3.org/2000/svg">
        <title>Vector Service Deployment Strategy</title>
        <desc>Vector service deployment strategy</desc>
        <defs>
            <linearGradient x1="100%" y1="50%" x2="0%" y2="50%" id="linearGradient-1">
                <stop stop-color="#00DEFF" offset="0%"></stop>
                <stop stop-color="#000000" offset="100%"></stop>
            </linearGradient>
            <linearGradient x1="100%" y1="50%" x2="0%" y2="50%" id="linearGradient-2">
                <stop stop-color="#00DEFF" offset="0%"></stop>
                <stop stop-color="#000000" offset="100%"></stop>
            </linearGradient>
            <linearGradient x1="100%" y1="50%" x2="0%" y2="50%" id="linearGradient-3">
                <stop stop-color="#00DEFF" offset="0%"></stop>
                <stop stop-color="#000000" offset="100%"></stop>
            </linearGradient>
            <linearGradient x1="100%" y1="50%" x2="0%" y2="50%" id="linearGradient-4">
                <stop stop-color="#00DEFF" offset="0%"></stop>
                <stop stop-color="#000000" offset="100%"></stop>
            </linearGradient>
            <linearGradient x1="100%" y1="50%" x2="0%" y2="50%" id="linearGradient-5">
                <stop stop-color="#00DEFF" offset="0%"></stop>
                <stop stop-color="#000000" offset="100%"></stop>
            </linearGradient>
            <linearGradient x1="100%" y1="50%" x2="0%" y2="50%" id="linearGradient-6">
                <stop stop-color="#00DEFF" offset="0%"></stop>
                <stop stop-color="#000000" offset="100%"></stop>
            </linearGradient>
            <linearGradient x1="100%" y1="50%" x2="0%" y2="50%" id="linearGradient-7">
                <stop stop-color="#00DEFF" offset="0%"></stop>
                <stop stop-color="#000000" offset="100%"></stop>
            </linearGradient>
            <linearGradient x1="100%" y1="50%" x2="0%" y2="50%" id="linearGradient-8">
                <stop stop-color="#00DEFF" offset="0%"></stop>
                <stop stop-color="#000000" offset="100%"></stop>
            </linearGradient>
        </defs>
        <g id="Diagram" stroke="none" stroke-width="1" fill="none" fill-rule="evenodd">
            <g id="Centralized-Service" transform="translate(-167.000000, 27.000000)">
                <g id="Vector" transform="translate(523.000000, 83.000000)">
                    <polygon id="Stroke-1" stroke-opacity="0.152671547" stroke="#000000" stroke-linecap="round" stroke-linejoin="round" points="0.584172226 98.4483011 94.8697999 137.444652 189.153507 98.4483011 94.8697999 59.4478225"></polygon>
                    <polygon id="Stroke-7" stroke-opacity="0.152671547" stroke="#000000" stroke-linecap="round" stroke-linejoin="round" points="94.6741156 0.517419741 189.614585 39.9444187 189.153699 98.4478883 94.8699919 59.4474098"></polygon>
                    <polygon id="Stroke-9" stroke-opacity="0.152671547" stroke="#000000" stroke-linecap="round" stroke-linejoin="round" points="0.92177077 39.4382353 0.92177077 98.4452742 94.8697999 59.3925125 94.6739236 0.517557328"></polygon>
                    <polygon id="Fill-11" fill-opacity="0.0739182692" fill="#000000" points="0.584172226 98.4483011 0.388295937 39.5183111 94.8697999 0.496781655 189.750738 39.9106548 189.439333 98.3107139 94.8697999 137.444652"></polygon>
                    <polygon id="Stroke-13" stroke-opacity="0.152671547" stroke="#000000" stroke-linecap="round" stroke-linejoin="round" points="0.584172226 98.4483011 0.388295937 39.5183111 94.8697999 0.496754138 189.750738 39.9106548 189.439256 98.3107139 94.8697999 137.444652"></polygon>
                    <g transform="translate(22.000000, 20.000000)">
                        <polygon id="Fill-21" fill-opacity="0.6" fill="#10E7FF" points="0.319983157 73.5464803 72.6640666 102.826696 145.00648 73.5464803 72.6640666 44.2631054"></polygon>
                        <polygon id="Fill-22" fill-opacity="0.6" fill="#10E7FF" points="-1.13686838e-13 29.2390165 -1.13686838e-13 73.5442075 73.4825621 102.830626 73.4825621 58.903512"></polygon>
                        <polygon id="Fill-23" fill-opacity="0.6" fill="#10E7FF" points="73.4825621 58.9038972 145.3612 29.619617 145.007482 73.5465767 73.4825621 102.830934"></polygon>
                        <polygon id="Fill-24" fill-opacity="0.6" fill="#10E7FF" points="72.5137613 0.0156015384 145.360365 29.6190392 145.006647 73.5461722 72.6640666 44.2629128"></polygon>
                        <polygon id="Fill-25" fill-opacity="0.6" fill="#10E7FF" points="-1.70530257e-13 29.2390165 -1.70530257e-13 73.5442075 72.6640666 44.2216939 72.5137613 0.0156015384"></polygon>
                        <polygon id="Fill-26" fill-opacity="0.6" fill="#10E7FF" points="0.319983157 73.5464803 0.169677916 29.2991113 72.6640666 -4.97379915e-13 145.464744 29.593807 145.225759 73.4431635 72.6640666 102.826696"></polygon>
                        <polygon id="Fill-21-Copy" fill-opacity="0.6" fill="#00C1D8" points="0.319983157 29.4164145 72.6640666 58.6966307 145.00648 29.4164145 72.6640666 0.133039574"></polygon>
                        <g id="Logo" transform="translate(80.000000, 64.000000)" fill="#FFFFFF">
                            <polygon id="Rectangle" points="27.8272992 0 32.7366051 2.78172071e-13 21.0449604 19.6525955 18.6192519 15.5857557"></polygon>
                            <polygon id="Triangle-Copy" transform="translate(11.630110, 9.774580) scale(1, -1) translate(-11.630110, -9.774580) " points="11.6301097 4.54747351e-13 23.2602194 19.5491608 17.4591095 19.5491608 11.7490773 9.9917933 6.0362812 19.5491608 0 19.5491608"></polygon>
                        </g>
                        <path d="M53.2745555,70.9341439 L56.2532789,70.9341439 L56.2532789,68.2391084 L53.2745555,68.2391084 L53.2745555,70.9341439 Z M49.7284562,70.9341439 L52.7071796,70.9341439 L52.7071796,68.2391084 L49.7284562,68.2391084 L49.7284562,70.9341439 Z M46.2532789,70.9341439 L49.2320023,70.9341439 L49.2320023,68.2391084 L46.2532789,68.2391084 L46.2532789,70.9341439 Z M42.7781016,70.9341439 L45.685903,70.9341439 L45.685903,68.2391084 L42.7781016,68.2391084 L42.7781016,70.9341439 Z M39.2320023,70.9341439 L42.2107257,70.9341439 L42.2107257,68.2391084 L39.2320023,68.2391084 L39.2320023,70.9341439 Z M42.7781016,67.6717325 L45.685903,67.6717325 L45.685903,64.9766971 L42.7781016,64.9766971 L42.7781016,67.6717325 Z M46.2532789,67.6717325 L49.2320023,67.6717325 L49.2320023,64.9766971 L46.2532789,64.9766971 L46.2532789,67.6717325 Z M49.7284562,67.6717325 L52.7071796,67.6717325 L52.7071796,64.9766971 L49.7284562,64.9766971 L49.7284562,67.6717325 Z M49.7284562,64.4093212 L52.7071796,64.4093212 L52.7071796,61.7142857 L49.7284562,61.7142857 L49.7284562,64.4093212 Z M65.756825,69.7284701 C65.756825,69.7284701 64.4802293,68.5227964 61.8561158,68.9483283 C61.5724278,66.8915907 59.3738463,65.6859169 59.3738463,65.6859169 C59.3738463,65.6859169 57.3171087,68.1681864 58.8064704,70.9341439 C58.3809385,71.1469098 57.6717186,71.4305978 56.6078888,71.4305978 L37.1043427,71.4305978 C36.7497328,72.7781155 36.7497328,81.7142857 46.5369669,81.7142857 C53.5582435,81.7142857 58.8064704,78.4518744 61.2887399,72.4944276 C64.9766832,72.7781155 65.756825,69.7284701 65.756825,69.7284701 Z" id="Shape" fill="#FFFFFF" fill-rule="nonzero"></path>
                    </g>
                    <polygon id="Stroke-3" stroke-opacity="0.152671547" stroke="#000000" stroke-linecap="round" stroke-linejoin="round" points="0.92177077 39.4382353 0.92177077 98.4452742 95.0192036 137.44988 95.0192036 78.9464108"></polygon>
                    <polygon id="Stroke-5" stroke-opacity="0.152671547" stroke="#000000" stroke-linecap="round" stroke-linejoin="round" points="95.0192036 78.9468235 189.615737 39.9449691 189.154851 98.4484387 95.0192036 137.450293"></polygon>
                </g>
                <g id="Lines" transform="translate(0.000000, 15.000000)" fill-rule="nonzero">
                    <path d="M519.637605,135.980212 C385.924638,111.157494 294.671349,86.5017984 245.842242,61.9774423 C194.739685,36.3112494 167.468951,16.2696335 164.060307,1.75792137 L163.8971,1.06309826 L166.739842,0.90353457 L166.903049,1.59835768 C170.217948,15.7109661 197.204429,35.5436808 247.892758,61.0018285 C296.364157,85.3465257 387.29171,109.914489 520.639925,134.670039 L523.644245,130.742968 L538.8971,138.758288 L516.628099,139.914061 L519.637605,135.980212 L519.637605,135.980212 Z" id="Line" fill="url(#linearGradient-1)"></path>
                    <path d="M519.087844,136.826818 C376.26864,118.925278 274.736051,96.1849153 214.477809,68.5754535 C151.816721,39.8650425 106.347514,21.7366406 78.153855,14.2104181 L76.8971001,13.8749308 L78.2749297,12.650903 L79.5316847,12.9863904 C108.002208,20.5865209 153.611088,38.7706098 216.441989,67.5588273 C276.282521,94.9769003 377.400781,117.624973 519.784502,135.472774 L521.872285,131.414895 L538.8971,138.528707 L516.995784,140.893008 L519.087844,136.826818 L519.087844,136.826818 Z" id="Line" fill="url(#linearGradient-2)"></path>
                    <path d="M518.871446,138.723957 C371.945373,126.309887 262.305358,107.457405 189.939101,82.1453141 C114.596291,55.7920914 69.6328883,40.3166741 55.0996731,35.7358524 L53.8971001,35.3568053 L55.4567522,34.1877451 L56.6593253,34.5667922 C71.2501315,39.1657664 116.217915,54.6426915 191.613454,81.0143576 C263.569795,106.183069 372.817719,124.968589 519.344927,137.349724 L520.763844,133.231454 L538.8971,139.65215 L517.449539,142.850903 L518.871446,138.723957 L518.871446,138.723957 Z" id="Line" fill="url(#linearGradient-3)"></path>
                    <path d="M518.844882,139.646695 C368.121553,133.408148 254.71441,118.948618 178.606146,96.2457529 C99.0233508,72.5064459 58.1135556,52.1348401 55.9836284,34.9696985 L55.8971001,34.2723648 L58.7493466,34.1877451 L58.8358749,34.8850788 C60.888972,51.4310445 101.272824,71.5407496 180.0943,95.0529572 C255.759832,117.623756 368.756988,132.03146 519.068464,138.25372 L519.738504,134.07921 L538.8971,139.719794 L518.173455,143.82985 L518.844882,139.646695 Z" id="Line" fill="url(#linearGradient-4)"></path>
                    <path d="M518.903815,139.644018 C332.129747,140.974125 202.327983,128.864821 129.444615,103.259728 C55.1181581,77.147655 13.8150737,83.3508251 3.91320425,121.972527 L3.73583994,122.664327 L0.897100094,122.491432 L1.0744644,121.799632 C11.2695534,82.034244 55.1579253,75.4427985 131.117423,102.128585 C203.381154,127.515989 332.642195,139.575224 518.846644,138.249916 L518.675282,134.071278 L538.8971,138.755512 L519.075472,143.82985 L518.903815,139.644018 L518.903815,139.644018 Z" id="Line" fill="url(#linearGradient-5)"></path>
                    <path d="M519.541794,274.967022 C385.885488,251.436359 294.663762,227.433841 245.842257,202.922642 C194.739694,177.266221 167.468957,157.232235 164.06031,142.726042 L163.8971,142.031466 L166.739841,141.871956 L166.903052,142.566531 C170.217949,156.673756 197.204426,176.498915 247.892749,201.947364 C296.342796,226.27207 387.22245,250.184887 520.497356,273.648995 L523.361423,269.698528 L538.8971,277.580167 L516.67258,278.924587 L519.541794,274.967022 L519.541794,274.967022 Z" id="Line-Copy-5" fill="url(#linearGradient-6)" transform="translate(351.397100, 210.398271) scale(1, -1) translate(-351.397100, -210.398271) "></path>
                    <path d="M519.087844,264.089976 C376.26864,246.188436 274.736051,223.448073 214.477809,195.838611 C151.816721,167.1282 106.347514,148.999798 78.153855,141.473576 L76.8971001,141.138089 L78.2749297,139.914061 L79.5316847,140.249548 C108.002208,147.849679 153.611088,166.033768 216.441989,194.821985 C276.282521,222.240058 377.400781,244.888131 519.784502,262.735931 L521.872285,258.678052 L538.8971,265.791865 L516.995784,268.156166 L519.087844,264.089976 L519.087844,264.089976 Z" id="Line-Copy-4" fill="url(#linearGradient-2)" transform="translate(307.897100, 204.035114) scale(1, -1) translate(-307.897100, -204.035114) "></path>
                    <path d="M518.850377,241.4942 C371.937708,229.902335 262.304421,211.426723 189.939089,186.045424 C114.596279,159.61981 69.6328773,144.101882 55.0996624,139.508477 L53.8971001,139.128392 L55.4567853,137.956166 L56.6593476,138.336251 C71.2501537,142.947859 116.217936,158.467299 191.613475,184.911407 C263.558363,210.14524 372.78785,228.553286 519.290507,240.113608 L520.609433,235.976428 L538.8971,242.309967 L517.528583,245.640377 L518.850377,241.4942 L518.850377,241.4942 Z" id="Line-Copy-3" fill="url(#linearGradient-7)" transform="translate(296.397100, 191.798271) scale(1, -1) translate(-296.397100, -191.798271) "></path>
                    <path d="M518.845363,240.474582 C368.126261,235.524021 254.718643,221.7121 178.606119,199.015248 C99.0233386,175.283562 58.1135488,154.918494 55.9836244,137.758882 L55.8971001,137.061804 L58.7493471,136.977219 L58.8358714,137.674297 C60.8889714,154.214978 101.27283,174.318235 180.094322,197.822904 C255.747339,220.382731 368.727647,234.143089 519.018857,239.08041 L519.538775,234.902424 L538.8971,240.375532 L518.324343,244.661429 L518.845363,240.474582 L518.845363,240.474582 Z" id="Line-Copy-2" fill="url(#linearGradient-8)" transform="translate(297.397100, 190.819324) scale(1, -1) translate(-297.397100, -190.819324) "></path>
                    <path d="M518.903815,190.549281 C332.129747,191.879388 202.327983,179.770084 129.444615,154.164991 C55.1181581,128.052918 13.8150737,134.256088 3.91320425,172.87779 L3.73583994,173.56959 L0.897100094,173.396695 L1.0744644,172.704896 C11.2695534,132.939507 55.1579253,126.348062 131.117423,153.033848 C203.381154,178.421252 332.642195,190.480487 518.846644,189.155179 L518.675282,184.976541 L538.8971,189.660775 L519.075472,194.735114 L518.903815,190.549281 L518.903815,190.549281 Z" id="Line-Copy" fill="url(#linearGradient-5)" transform="translate(269.897100, 165.856166) scale(1, -1) translate(-269.897100, -165.856166) "></path>
                    <polygon id="Line-5" fill="#10E7FF" points="985.096539 124.326316 1017.29654 143.415789 985.096539 162.505263 985.096539 146.842105 695.296539 146.842105 695.296539 139.989474 985.096539 139.989474"></polygon>
                </g>
                <g id="Receive-Text" transform="translate(329.000000, 0.000000)" fill="#000000">
                    <foreignObject x="10" y="0" width="350" height="200">
                        <div xmlns="http://www.w3.org/1999/xhtml" align="left" style={{fontSize: '0.9em'}}>
                          <div><strong>1. {receiveTitle}</strong></div>
                          <div>{receiveDescription}</div>
                        </div>
                    </foreignObject>
                    <polygon id="Line-5" fill-rule="nonzero" points="1.83047945 90.015411 1.83047945 5.01541096 0.109589041 5.01541096 0.109589041 90.015411"></polygon>
                </g>
                <g id="Transform-Text" transform="translate(364.000000, 232.000000)" fill="#000000">
                    <foreignObject x="-110" y="40" width="350" height="200">
                      <div xmlns="http://www.w3.org/1999/xhtml" align="right" style={{fontSize: '0.9em'}}>
                        <div><strong>2. Vector processes data</strong></div>
                        <div>Vector parses, transforms, and enriches data.</div>
                      </div>
                    </foreignObject>
                    <polygon id="Line-5" fill-rule="nonzero" points="255 85 255 0 253 0 253 85"></polygon>
                </g>
                <g id="Forward-Text" transform="translate(721.000000, 177.000000)" fill="#000000">
                    <foreignObject x="-20" y="17" width="250" height="200">
                      <div xmlns="http://www.w3.org/1999/xhtml" align="right" style={{fontSize: '0.9em'}}>
                        <div><strong>3. {forwardTitle}</strong></div>
                        <div>{forwardDescription}</div>
                      </div>
                    </foreignObject>
                    <polygon id="Line-5" fill-rule="nonzero" points="242 85 242 0 240 0 240 85"></polygon>
                </g>
            </g>
        </g>
    </svg>
  );
}

export default ServiceDiagram;
