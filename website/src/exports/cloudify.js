let $ = null;
let SVG = null;

if (typeof document !== 'undefined') {
  $ = require('cash-dom');
  SVG = require('svg.js');
}

export default function cloudify() {
  var SVG_INSTANCE = SVG('component-canvas').size('100%', '100%')
  var CANVAS = SVG_INSTANCE.group();

  // ----------------------------------------
  // PAGE RESIZING
  // Only runs on initial load, not window resize

  if ($(window).width() >= 1000) {
    var CANVAS_SIZE = 1000;
    var CANVAS_OFFSET_X = 500;
    var CANVAS_OFFSET_Y = 500;
    $('.components').addClass('initial-large');
  } else {
    var CANVAS_SIZE = 800;
    var CANVAS_OFFSET_X = 400;
    var CANVAS_OFFSET_Y = 400;
    $('.components').addClass('initial-small');
  }

  function setCanvasOffsets() {
    CANVAS_OFFSET_X = $('.components').innerWidth() / 2;
    CANVAS_OFFSET_Y = $('.components').innerHeight() / 2;;
    CANVAS.transform({
      x: CANVAS_OFFSET_X,
      y: CANVAS_OFFSET_Y
    });
  }
  setCanvasOffsets();
  $(window).on('resize', setCanvasOffsets);

  // ----------------------------------------
  // VECTORS

  var Vector = function(_x, _y) {

    this.x = _x || 0;
    this.y = _y || 0;

    this.dist = function(vector) {
      var dx = this.x - vector.x;
      var dy = this.y - vector.y;
      return Math.sqrt(dx * dx + dy * dy);
    };

    this.minus = function(vector) {
      return new Vector(
        this.x - vector.x,
        this.y - vector.y);
    };

    this.scale = function(scale) {
      this.x *= scale;
      this.y *= scale;
    };

    this.apply = function(vector) {
      this.x += vector.x;
      this.y += vector.y;
    };

    this.length = function() {
      return Math.sqrt(this.x * this.x + this.y * this.y);
    }

  };

  // ----------------------------------------
  // NODES

  var NODES = [];
  if ($(window).width() >= 1000) {
    var RADIUS_SMALL = 25;
    var RADIUS_MEDIUM = 45;
    var RADIUS_LARGE = 60;
  } else {
    var RADIUS_SMALL = 18;
    var RADIUS_MEDIUM = 38;
    var RADIUS_LARGE = 50;
  }
  var RADIUS_ANIMATION_SPEED = 0.15;
  var TEXT_ANIMATION_SPEED = 0.10;
  var GRAY = "#bdbecf"
  var GRAY_TXT = "#8183A4"
  var Node = function(_home, _label) {
    this.home = _home; // Home position
    this.pos = new Vector(_home.x, _home.y); // Current position
    this.radius = RADIUS_SMALL;
    this.targetRadius = this.radius;
    this.textOpacity = 0;
    this.targetTextOpacity = 0;
    this.label = _label;
    this.defaultStyle = 'small';
    this.currentStyle = 'small';
    this.url = null;
    NODES.push(this);

    // Initialize
    this.svgGroup = CANVAS.group();
    this.svgGroup.addClass('item')
    this.svgCircle = this.svgGroup.circle()
    this.svgCircle.fill("#fff").stroke(GRAY);
    this.svgText = this.svgGroup.text(this.label);
    this.svgText.font({
      family: 'open-sans, helvetica neue, helvetica',
      size: 12,
      anchor: 'middle',
      leading: '1.2em',
    })
    if (this.label.indexOf('\n') == -1) {
      this.svgText.transform({
        y: -9,
      });
    } else {
      this.svgText.transform({
        y: -16,
      });
    }

    // Click events:
    var self = this;
    this.svgGroup.click(function() {
      if (self.url) {
        window.location = self.url;
      }
    })

    this.setStyle = function(style) {

      if (this.currentStyle == style) return;

      var targetStyle = style;
      if (targetStyle == 'default') targetStyle = this.defaultStyle;

      if (targetStyle == 'large') {
        this.targetRadius = RADIUS_LARGE;
        this.targetTextOpacity = 1;
        this.svgGroup.addClass('large');
      } else if (targetStyle == 'large-hover') {
        this.targetRadius = RADIUS_LARGE;
        this.targetTextOpacity = 1;
        this.svgGroup.addClass('large');
      } else if (targetStyle == 'medium') {
        this.targetRadius = RADIUS_MEDIUM;
        this.targetTextOpacity = 1;
        this.svgGroup.removeClass('large');
      } else if (targetStyle == 'small') {
        this.targetRadius = RADIUS_SMALL;
        this.targetTextOpacity = 0;
        this.svgGroup.removeClass('large');
      }

      this.currentStyle = targetStyle;

    }

    this.draw = function() {
      // size
      this.radius += (this.targetRadius - this.radius) * RADIUS_ANIMATION_SPEED;
      var d = this.radius * 2;
      this.svgCircle.size(d, d);
      // text
      this.textOpacity += (this.targetTextOpacity - this.textOpacity) * TEXT_ANIMATION_SPEED;
      this.svgText.opacity(this.textOpacity);
      // position
      this.svgGroup.transform({
        x: this.pos.x,
        y: this.pos.y,
      })
    }
  }

  var EDGES = [];
  var Edge = function(_n1, _n2) {
    this.n1 = _n1;
    this.n2 = _n2;
    this.homeLength = this.n1.home.dist(this.n2.home);
    EDGES.push(this);

    this.length = function() {
      return this.n1.pos.dist(this.n2.pos);
    };
  };

  // ----------------------------------------
  // INITIAL NODE & EDGE CREATION

  var HOME_RADIUS = 0.3 * CANVAS_SIZE;
  var RADIUS_VARIANCE = 0.1;
  var INITIAL_RADIUS_SCALE = 0.1;

  var elements = $(".components li");
  elements.each(function(i, e) {

    var el = $(e);
    var p = (i / elements.length) * Math.PI * 2;
    var r = HOME_RADIUS + Math.random() * RADIUS_VARIANCE - RADIUS_VARIANCE / 2;

    var home = new Vector(
      Math.sin(p) * r,
      Math.cos(p) * r);

    var label = el.text();
    label = label.replace(" ", "\n");
    var n = new Node(home, label);

    n.pos = new Vector(
      Math.sin(p) * r * INITIAL_RADIUS_SCALE,
      Math.cos(p) * r * INITIAL_RADIUS_SCALE);

    if (el.hasClass('medium')) {
      n.defaultStyle = 'medium'
    } else if (el.hasClass('large')) {
      n.defaultStyle = 'large'
    }

    var url = $("a", e).attr('href');
    if (url) {
      n.url = url;
    }

    n.setStyle('default');
    n.draw();

  });

  // Create Edges
  var EDGE_SPAN = 12;
  for (var i = 0; i < NODES.length; i++) {
    var n1 = NODES[i]
    for (var j = 1; j < EDGE_SPAN; j++) {
      var n2 = NODES[(i + j) % NODES.length];
      new Edge(n1, n2);
    }
  }

  // ----------------------------------------
  // FORCES & ANIMATION

  var EDGE_FORCE = 0.3;
  var EDGE_PADDING = 30;

  function applyEdgeForces() {
    for (var e in EDGES) {
      var edge = EDGES[e];

      // Edge lengths
      var min_length = edge.n1.radius + edge.n2.radius;
      min_length += EDGE_PADDING;
      var target_length = min_length;
      var length = edge.length();

      // Collisions
      if (length < min_length) {
        // Calculate forces
        var force = edge.n1.pos.minus(edge.n2.pos);
        var stretch = length - min_length;
        force.scale(stretch / length * EDGE_FORCE);
        // Apply forces
        edge.n2.pos.apply(force);
        force.scale(-1);
        edge.n1.pos.apply(force);
      }
    }
  }

  function applyHomeForces() {
    var HOME_FORCE = 0.1;
    for (var n in NODES) {
      var node = NODES[n];
      var diff = node.home.minus(node.pos);
      diff.scale(HOME_FORCE);
      node.pos.apply(diff);
    }
  }

  function redraw() {
    for (var n in NODES) {
      NODES[n].draw();
    }
  }

  var ANIMATION_INTERVAL = null;
  var ANIMATION_TIMEOUT = null;

  function animate() {
    // Timeout since after a period of time the animation effectively stops.
    // This helps with performance.
    if (ANIMATION_TIMEOUT != null) clearTimeout(ANIMATION_TIMEOUT);
    ANIMATION_TIMEOUT = setTimeout(function() {
      stopAnimation();
    }, 4000);

    startAnimation();
  }

  function startAnimation() {
    if (ANIMATION_INTERVAL != null) return;
    ANIMATION_INTERVAL = setInterval(function() {
      applyHomeForces();
      applyEdgeForces();
      redraw();
    }, 1000 / 30);
  }

  function stopAnimation() {
    if (ANIMATION_INTERVAL == null) return;
    clearInterval(ANIMATION_INTERVAL);
    ANIMATION_INTERVAL = null;
  }

  animate();

  // ----------------------------------------
  // MOUSE MOVEMENTS

  $("#component-canvas").on('mousemove', function(e) {
    animate();

    var offset = $("#component-canvas").offset();
    var mouse = new Vector(
      e.pageX - offset.left - CANVAS_OFFSET_X,
      e.pageY - offset.top - CANVAS_OFFSET_Y
    );

    for (var n in NODES) {
      var node = NODES[n];
      var dist = mouse.dist(node.pos);

      if (dist < 100) {
        node.setStyle('large-hover');
      } else if (dist < 210 && node.defaultStyle != 'large') {
        node.setStyle('large');
      } else {
        node.setStyle('default');
      }
    }

  });
};
