import React, { useState, useEffect } from 'react';

import Select from 'react-select';

import axios from 'axios';
import classnames from 'classnames';
import sortBy from 'lodash/sortBy';

import './styles.css';

const COLUMN_CHART_HEIGHT = 205;
const DEFAULT_METRIC_SLUG = 'throughput_avg';
const DEFAULT_TEST_SLUG = 'file_to_tcp_performance';
const WHITE_LISTED_METRICS = [
  'load_avg_1m',
  'mem_used_max',
  'throughput_avg'
];

function buildMetricOptions(metrics) {
  return metrics.map(metric => ({value: metric.slug, label: metric.name}));
}

function buildSubjectOptions(subjects) {
  return subjects.map(subject => ({value: subject.slug, label: subject.name}));
}

function buildTestOptions(tests) {
  return tests.map(test => ({value: test.slug, label: test.name}));
}

function fetchMeasurements(columns, row, metrics, allMeasurements) {
  let metric = row.__type == "metric" ? row : metrics[0];
  let measurements = columns.map(column => fetchMeasurement(allMeasurements, row, column, metric));

  let sortedValues = measurements.
    filter(measurement => measurement !== null).
    map(measurement => measurement.value).
    sort((a, b) => (a - b));

  if (metric.sort == "desc")
    sortedValues = sortedValues.reverse();

  let winningValue = sortedValues[0];

  return measurements.map(measurement => {
    if (measurement) {
      let percent = measurement.value > winningValue ?
        ((measurement.value / winningValue) * 100) :
        ((1 - (measurement.value / winningValue)) * 100)

      measurement.minValue = Math.min(...sortedValues);
      measurement.maxValue = Math.max(...sortedValues);
      measurement.percent = Math.round(percent);
      measurement.percentWord = measurement.value > winningValue ? 'more' : 'less';
      measurement.place = sortedValues.indexOf(measurement.value) + 1;
    }

    return measurement;
  });
}

function fetchMeasurement(measurements, row, column, metric) {
  let test = row.__type == "test" ? row : null;
  let subject = column.__type == "subject" ? column : null;
  let versions = column.__type == "version" ? [column] : column.versions.reverse();

  for(let version of versions) {
    let measurement =
      measurements.find(measurement => (
        (!test || measurement.test == test.slug) &&
        (!subject || measurement.subject == subject.slug) &&
        measurement.metric == metric.slug &&
        measurement.version == version.slug
      ));

    if (measurement)
      return measurement;
  }

  return null;
}

function filterMetrics(metrics, {subjectSlug, testSlug}) {
  let filteredMetrics = metrics.filter(metric => WHITE_LISTED_METRICS.includes(metric.slug));

  if (subjectSlug) {
    filteredMetrics = filteredMetrics.filter(metric => metric.subjects.includes(subjectSlug));
  }

  if (testSlug) {
    filteredMetrics = filteredMetrics.filter(metric => metric.tests.includes(testSlug));
  }

  return filteredMetrics;
}

function filterSubjects(subjects, {metricSlug, testSlug}) {
  let filteredSubjects = subjects;

  if (metricSlug) {
    filteredSubjects = filteredSubjects.filter(subject => subject.metrics.includes(metricSlug));
  }

  if (testSlug) {
    filteredSubjects = filteredSubjects.filter(subject => subject.tests.includes(testSlug));
  }

  return filteredSubjects;
}

function filterTests(tests, {metricSlug, subjectSlug}) {
  let filteredTests = tests;

  if (metricSlug) {
    filteredTests = filteredTests.filter(test => test.metrics.includes(metricSlug));
  }

  if (subjectSlug) {
    filteredTests = filteredTests.filter(test => test.subjects.includes(subjectSlug));
  }

  return tests;
}

function Prompt({value}) {
  return (
    <div className="performance-tests__prompt">
      {value}
    </div>
  );
}

function Column({obj, onClick}) {
  switch (obj.__type) {
    case 'subject':
      return (
        <th title={`version ${obj.versions.reverse()[0].name}`}>
          {obj.name}
        </th>
      );

    case 'version':
      return (
        <th>
          {obj.name}
        </th>
      )

    default:
      throw new Error("Unknown column object");
  }
}

function MeasurementBar({value: measurement}) {
  if (measurement) {
    let height = Math.round(COLUMN_CHART_HEIGHT * (measurement.value / measurement.maxValue));

    if (measurement.place == 1) {
      return (
        <td className="bar" title="Winner">
          <div>{measurement.human_value}</div>
          <div className="bar passed" style={{height: `${height}px`}}></div>
        </td>
      );
    } else {
      return (
        <td className="bar" title={`This subject lost, it was ${measurement.percent}% worse than the winner.`}>
          <div>{measurement.human_value}</div>
          <div className="bar lost" style={{height: `${height}px`}}></div>
        </td>
      );
    }
  } else {
    return (<td>unsupported</td>);
  }
}

function MeasurementValue({columnChart, value: measurement}) {
  if (measurement) {
    if (measurement.place == 1) {
      return (
        <td className="passed" title="Winner">
          <i className="feather icon-award"></i>{measurement.human_value}
        </td>
      );
    } else {
      return (
        <td
          className={classnames('lost', `place-${measurement.place}`)}
          title={`This subject lost, it was ${measurement.percent}% worse than the winner.`}>
          {measurement.human_value}
        </td>
      );
    }
  } else {
    return (
      <td className="not-applicable" title="This subject lacked the features neecssary to be involved in this test.">n/a</td>
    );
  }
}

function RowDescription({test}) {
  return (
    <td className="description">
      <div className="label">Description</div>
      <div className="text">{test.description}</div>
      <div className="links">
        <div><a href={`https://github.com/timberio/vector-test-harness/tree/master/cases/${test.slug}`} target="_blank">Learn more&hellip;</a></div>
      </div>
    </td>
  );
}

function RowLink({value: row, onClick}) {
  return (
    <td>
      <span className="link" onClick={() => onClick(row)}>{row.name}</span>
    </td>
  );
}

function Compare({measurements, metrics, onColumnClick, onRowClick, subjects, tests}) {
  let columns = subjects.length > 1 ? subjects : subjects[0].versions;
  let rows = tests.length > 1 ? tests : metrics;

  return (
    <div className="table-responsive">
      <table className="comparison">
        <thead>
          <tr>
            <th></th>
            {columns.map((column, columnIdx) => (
              <Column key={columnIdx} obj={column} onClick={() => onColumnClick(column)} />
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, rowIdx) => (
            <tr key={rowIdx}>
              {rows.length == 1 ?
                <RowDescription test={tests[0]} /> :
                <RowLink value={row} onClick={onRowClick} />
              }
              {fetchMeasurements(columns, row, metrics, measurements).map((measurement, measurementIdx) => {
                if (rows.length == 1) {
                  return <MeasurementBar key={measurementIdx} value={measurement} />
                } else {
                  return <MeasurementValue key={measurementIdx} value={measurement} />
                }
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function handleColumnClick(column, setSubjectSlug) {
  switch (column.__type) {
    case 'subject':
      setSubjectSlug(column.slug);
      break;

    default:
      throw new Error('Unknown column type');
  }
}

function handleRowClick(row, setTestSlug, setMetricSlug) {
  switch (row.__type) {
    case 'test':
      setTestSlug(row.slug);
      break;

    case 'metric':
      setMetricSlug(row.slug);
      break;

    default:
      throw new Error('Unknown row type');
  }
}

function PerformanceTests({}) {
  const [data, setData] = useState(null);
  const [testSlug, setTestSlug] = useState(DEFAULT_TEST_SLUG);
  const [subjectSlug, setSubjectSlug] = useState(null);
  const [metricSlug, setMetricSlug] = useState(DEFAULT_METRIC_SLUG);

  useEffect(() => {
    async function fetchData() {
      const result = await axios(
        `https://test-results.vector.dev/summaries/latest.json`,
      );

      setData(result.data.performance_tests);
    }
    fetchData();
  }, []);

  if (data !== null) {
    let tests = filterTests(data.tests, {metricSlug: metricSlug, subjectSlug: subjectSlug});
    let subjects = filterSubjects(data.subjects, {metricSlug: metricSlug, testSlug: testSlug});
    let metrics = filterMetrics(data.metrics, {subjectSlug: subjectSlug, testSlug: testSlug});
    let measurements = data.measurements;

    const testOptions = buildTestOptions(tests);
    const subjectOptions = buildSubjectOptions(subjects);
    const metricOptions = buildMetricOptions(metrics);

    if (testSlug) {
      tests = tests.filter(testObj => testObj.slug == testSlug);
      measurements = measurements.filter(measurementObj => measurementObj.test == testSlug);
    }

    if (subjectSlug) {
      subjects = subjects.filter(subjectObj => subjectObj.slug == subjectSlug);
      measurements = measurements.filter(measurementObj => measurementObj.subject == subjectSlug);
    }

    if (metricSlug) {
      metrics = metrics.filter(metricObj => metricObj.slug == metricSlug);
      measurements = measurements.filter(measurementObj => measurementObj.metric == metricSlug);
    }

    return (
      <div className="performance-tests">
        <div className="performance-tests--switcher">
          <Select
            className="react-select-container"
            classNamePrefix="react-select"
            options={testOptions}
            isClearable={true}
            placeholder="Select a test..."
            value={testOptions.find(option => option.value == testSlug)}
            onChange={(selectedOption) => setTestSlug(selectedOption ? selectedOption.value : null)} />

          <div></div>

          <Select
            className="react-select-container"
            classNamePrefix="react-select"
            options={metricOptions}
            isClearable={true}
            placeholder="Select a metric..."
            value={metricOptions.find(option => option.value == metricSlug)}
            onChange={(selectedOption) => setMetricSlug(selectedOption ? selectedOption.value : null)} />
        </div>

        {(metricSlug || testSlug) ?
          <Compare
            measurements={measurements}
            metrics={metrics}
            subjects={subjects}
            tests={tests}
            onColumnClick={(column) => handleColumnClick(column, setSubjectSlug)}
            onRowClick={(row) => handleRowClick(row, setTestSlug, setMetricSlug)} /> :
          <Prompt value="Please select a test or metric" />
        }
      </div>
    );
  } else {
    return (
      <div className="performance-tests">
        <Prompt value="Loading..." />
      </div>
    );
  }
}

export default PerformanceTests;