import { resolve } from '../Cargo.toml';
import $ from 'jquery';
import beautify from 'json-beautify';

const initialEvent = {
  foo: "bar"
};

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

  const html = JSON.stringify(result);

  $('#event').css('display', 'block').html(html);
  $('#program').val('');
}

$(() => {
  getResult("", initialEvent);

  $('#program').on('keypress', (e) => {
    if (e.key == 'Enter') {
      var event = $('#event').text();
      var program = $('#program').val();
      getResult(program, JSON.parse(event));
    }
  });
});
