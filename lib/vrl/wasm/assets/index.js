import { resolve } from '../Cargo.toml';
import $ from 'jquery';

const initialEvent = {
  message: "bar=baz foo=bar",
  timestamp: "2021-03-02T18:51:01.513+00:00"
};

const initialProgram = `
  . |= parse_key_value!(string!(.message))
  del(.message)
  .id = uuid_v4()
`;

const getResult = (program, event) => {
  const input = {
    program: program,
    event: event
  }

  const vrlResult = resolve(input);
  var result;

  if (vrlResult.result) {
    result = vrlResult.result;
  } else {
    result = vrlResult.error;
  }

  if (vrlResult.output) {
    const json = JSON.stringify(vrlResult.output);
    $('#output-box').css('display', 'block');
    $('#output').text(json);
  }

  const html = JSON.stringify(result, null, 2);

  $('#event').css('display', 'block').html(html);
  $('#program').val('');
}

$(() => {
  getResult(initialProgram, initialEvent);

  $('#program').on('keypress', (e) => {
    if (e.key == 'Enter') {
      var event = $('#event').text();
      var program = $('#program').val();
      getResult(program, JSON.parse(event));
    }
  });
});
