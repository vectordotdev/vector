export default function buildRows(xAxis, yAxis, metrics, measurements) {
  return yAxis.map(yAxisItem => {
    let row = xAxis.map(xAxisItem => {
      let measurement = fetchMeasurement(measurements, yAxisItem, xAxisItem);
      let metric = measurement ? metrics.find(metric => (metric.slug == measurement.metric)) : null;

      return {
        _type: '__cell',
        measurement: measurement,
        metric: metric,
        xAxisItem: xAxisItem,
        yAxisItem: yAxisItem
      }
    });

    return assignRankMetadata(row);
  });
}

function fetchMeasurement(measurements, yAxisItem, xAxisItem) {
  let test = yAxisItem.__type == "test" ? yAxisItem : null;
  let subject = xAxisItem.__type == "subject" ? xAxisItem : null;
  let metric = yAxisItem._type == "metric" ? yAxisItem : null;
  let versions = xAxisItem == "version" ? [xAxisItem] : subject.versions.reverse();

  for(let version of versions) {
    let measurement = measurements.find(measurement => (
      (!test || measurement.test == test.slug) &&
      (!subject || measurement.subject == subject.slug) &&
      (!metric || measurement.metric == metric.slug) &&
      (!version || measurement.version == version.slug)
    ));

    if (measurement)
      return measurement;
  }

  return null;
}

function assignRankMetadata(row) {
  let sortedValues = row.
    filter(cell => cell.measurement).
    map(cell => cell.measurement.value).
    sort((a, b) => (a - b));

  let metric = row.find(cell => cell.metric).metric;

  if (metric.sort == "desc")
    sortedValues = sortedValues.reverse();

  let maxValue = Math.max(...sortedValues);

  return row.map(cell => {
    if (cell.measurement) {
      cell.percent = cell.measurement.value / maxValue;
      cell.place = sortedValues.indexOf(cell.measurement.value) + 1;
    }

    return cell;
  });
}
