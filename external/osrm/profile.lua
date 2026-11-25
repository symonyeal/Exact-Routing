-- The benchmark's directed graph, with duration from its integer-millisecond tag.
api_version = 4

function setup()
  return {
    properties = {
      weight_name = 'duration',
      weight_precision = 1,
      continue_straight_at_waypoint = false,
      use_turn_restrictions = false,
      u_turn_penalty = 0,
      traffic_light_penalty = 0
    }
  }
end

function process_way(profile, way, result)
  local ms = tonumber(way:get_value_by_key('arc_ms'))
  if not ms or ms <= 0 then return end
  result.forward_mode = mode.driving
  result.backward_mode = mode.inaccessible
  result.duration = ms / 1000
  result.weight = ms / 1000
  result.is_startpoint = true
end

function process_turn(profile, turn)
  turn.duration = 0
  turn.weight = 0
end

return { setup = setup, process_way = process_way, process_turn = process_turn }
