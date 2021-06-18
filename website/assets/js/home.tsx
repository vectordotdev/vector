import React from "react";
import ReactDOM from "react-dom";
import { useInterval } from "react-use";
import { useSpring, animated } from "react-spring";
import { geoOrthographic, geoPath, geoDistance } from "d3-geo";
import { feature } from "topojson-client";

// types
import { Topology } from "@types/topojson-specification";

// countryData
import countryData from "./countries.json";
import markerData from "./markers.json";

interface IGlobeProps {
  size?: number;
  lat?: number;
  long?: number;
  rotation?: number;
  onClick?: () => void;
}

const markers: GeoJSON.Point[] = markerData.map((m) => ({
  type: "Point",
  coordinates: [m.coordinates[0], m.coordinates[1]],
}));

const topo: unknown = countryData;
const topology = topo as Topology;
const geojson = feature(
  topology,
  topology.objects.land
) as GeoJSON.FeatureCollection;
const countries = geojson.features;

export const Globe: React.FC<IGlobeProps> = animated(
  ({ size = 500, lat = 0, long = 0, rotation = 0, onClick }: IGlobeProps) => {
    const svgRef = React.useRef(null);

    // create geo projection to render paths
    const projection = React.useMemo(() => {
      return geoOrthographic()
        .translate([size / 2, size / 2])
        .scale(size / 2)
        .clipAngle(90)
        .rotate([rotation, 0]);
    }, [size, rotation, lat, long]);

    // return coordinates based on the projection
    const pathgen = geoPath().projection(projection);

    // find the center coordinates of the globe
    const center: [number, number] = [size / 2, size / 2];

    return (
      <div>
        <svg width={size} height={size} ref={svgRef} onClick={onClick}>
          <circle
            cx={size / 2}
            cy={size / 2}
            r={size / 2}
            style={{ cursor: "pointer" }}
            className="globe"
          />
          <g style={{ pointerEvents: "none" }}>
            {countries.map((d, i) => (
              <path key={`path-${i}`} d={pathgen(d) || ""} />
            ))}
            {markers.map((m, i) => {
              const coordinates: [number, number] = [
                m.coordinates[0],
                m.coordinates[1],
              ];
              const position = projection(coordinates);
              const x = position ? position[0] : undefined;
              const y = position ? position[1] : undefined;
              const invertedCoordinates =
                projection.invert && projection.invert(center);

              // hide the marker if rotated out of view
              const hideMarker =
                geoDistance(coordinates, invertedCoordinates || [0, 0]) > 1.4;

              return (
                <circle
                  key={`marker-${i}`}
                  className="marker"
                  cx={x}
                  cy={y}
                  r={7}
                  fillOpacity={0.4}
                  fill={hideMarker ? "none" : "rgba(0,255,135,1)"}
                  stroke={hideMarker ? "none" : "rgba(0,255,182,0.5)"}
                  strokeWidth={1}
                />
              );
            })}
          </g>
        </svg>
      </div>
    );
  }
);

export const RotatingGlobe: React.FC<{ size: number; duration?: number }> = ({
  size = 500,
  duration = 50000,
}) => {
  // globe positioning state
  const [orientation, setOrientation] = React.useState({
    lat: 0,
    long: 0,
    degrees: 0,
  });

  // set up spring animation
  const { lat, long, rotation } = useSpring({
    lat: orientation.lat,
    long: orientation.long,
    rotation: orientation.degrees,
    config: {
      duration,
    },
  });

  // globe click handler
  const onClick = React.useCallback(() => {}, []);

  // kick off the initial rotation
  React.useEffect(() => {
    setOrientation({ lat: 0, long: 0, degrees: 360 });
  }, []);

  // restart the animation every {duration} miliseconds
  useInterval(() => {
    setOrientation({
      lat: Math.floor(Math.random() * size),
      long: Math.floor(Math.random() * size),
      degrees: orientation.degrees + 360,
    });
  }, duration);

  return (
    <Globe
      size={size}
      lat={lat}
      long={long}
      rotation={rotation}
      onClick={onClick}
    />
  );
};

ReactDOM.render(<RotatingGlobe size={900} />, document.getElementById("globe"));
