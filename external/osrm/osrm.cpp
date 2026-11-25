// Native libosrm queries, segment-speed feeds and a read-back audit of engine units.
// INPUT has the binary layout documented by src/bin/scale.rs (export).
#include "osrm/osrm.hpp"
#include "osrm/engine_config.hpp"
#include "osrm/route_parameters.hpp"
#include "osrm/json_container.hpp"
#include "extractor/files.hpp"
#include "extractor/packed_osm_ids.hpp"
#include "extractor/segment_data_container.hpp"
#include "util/coordinate_calculation.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <limits>
#include <sstream>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>
#include <sys/resource.h>

using Clock = std::chrono::steady_clock;
using osrm::json::Array;
using osrm::json::Object;

struct Input {
    std::uint64_t n, m, q;
    std::vector<unsigned> tail, head, pairs;
    std::vector<float> lat, lon;
    std::vector<std::vector<unsigned>> metric{8}, ref{8};

    template<class T> static std::vector<T> read(std::ifstream &in, std::size_t n) {
        std::vector<T> v(n);
        if (!in.read(reinterpret_cast<char *>(v.data()), sizeof(T) * n))
            throw std::runtime_error("truncated benchmark input");
        return v;
    }
    explicit Input(const char *path) {
        std::ifstream in(path, std::ios::binary);
        n = read<std::uint64_t>(in, 1)[0];
        m = read<std::uint64_t>(in, 1)[0];
        tail = read<unsigned>(in, m); head = read<unsigned>(in, m);
        lat = read<float>(in, n); lon = read<float>(in, n);
        for (auto &w : metric) w = read<unsigned>(in, m);
        q = read<std::uint64_t>(in, 1)[0];
        pairs = read<unsigned>(in, q * 2);
        for (auto &d : ref) d = read<unsigned>(in, q);
    }
    osrm::util::Coordinate xy(unsigned v) const {
        return {osrm::util::FloatLongitude{lon.at(v)}, osrm::util::FloatLatitude{lat.at(v)}};
    }
};

static std::uint64_t key(unsigned u, unsigned v) { return (std::uint64_t(u) << 32) | v; }

// Use OSRM's own quantization and distance routine, also used by its updater.
static void segments(const Input &g, const std::string &dir) {
    std::vector<std::ofstream> files;
    for (int j = 0; j < 8; ++j) {
        files.emplace_back(dir + "/slot-" + std::to_string(j) + ".csv");
        if (!files.back()) throw std::runtime_error("cannot write segment feed");
        files.back() << std::setprecision(17);
    }
    for (std::size_t a = 0; a < g.m; ++a) {
        double metres = osrm::util::coordinate_calculation::greatCircleDistance(g.xy(g.tail[a]), g.xy(g.head[a]));
        if (metres <= 0) throw std::runtime_error("OSRM cannot load a zero-length directed segment");
        for (int j = 0; j < 8; ++j)
            files[j] << g.tail[a] + 1 << ',' << g.head[a] + 1 << ','
                     << metres * 3600.0 / g.metric[j][a] << '\n';
    }
}

// Read actual stored OSRM geometry after contract/customize, preserving direction.
static void audit(const Input &g, const std::string &graph, int slot) {
    std::unordered_map<std::uint64_t, unsigned> expected;
    for (std::size_t a = 0; a < g.m; ++a) {
        const auto k = key(g.tail[a], g.head[a]);
        const unsigned ds = std::max(1u, (g.metric[slot][a] + 50) / 100);
        auto [it, inserted] = expected.emplace(k, ds);
        if (!inserted) throw std::runtime_error("parallel directed arcs are ambiguous in OSRM segment feeds");
    }
    osrm::extractor::SegmentDataContainer data;
    osrm::extractor::PackedOSMIDs ids;
    std::vector<osrm::util::Coordinate> coords;
    osrm::extractor::files::readNodes(graph + ".nbg_nodes", coords, ids);
    osrm::extractor::files::readSegmentData(graph + ".geometry", data);
    std::unordered_set<std::uint64_t> seen;
    auto check = [&](const auto &nodes, const auto &weights, const auto &durations) {
        for (std::size_t i = 0; i + 1 < nodes.size(); ++i) {
            if (weights[i] == INVALID_SEGMENT_WEIGHT) continue;
            const unsigned u = static_cast<std::uint64_t>(ids[nodes[i]]) - 1;
            const unsigned v = static_cast<std::uint64_t>(ids[nodes[i + 1]]) - 1;
            auto k = key(u, v);
            auto it = expected.find(k);
            if (it == expected.end() || static_cast<unsigned>(weights[i]) != it->second
                || static_cast<unsigned>(durations[i]) != it->second)
                throw std::runtime_error("OSRM read-back changed arc direction or decisecond weight");
            seen.insert(k);
        }
    };
    for (unsigned i = 0; i < data.GetNumberOfGeometries(); ++i) {
        check(data.GetForwardGeometry(i), data.GetForwardWeights(i), data.GetForwardDurations(i));
        check(data.GetReverseGeometry(i), data.GetReverseWeights(i), data.GetReverseDurations(i));
    }
    if (seen.size() != expected.size()) throw std::runtime_error("OSRM read-back lost directed arcs");
    std::cout << "{\"kind\":\"industry_conversion\",\"engine\":\"osrm\",\"slot\":" << slot
              << ",\"arcs\":" << seen.size() << ",\"unit_ms\":100,\"wrong\":0}\n";
}

static std::vector<unsigned> path(osrm::engine::api::ResultT &result) {
    auto &json = result.get<Object>();
    auto &route = json.values.at("routes").get<Array>().values.at(0).get<Object>();
    auto &leg = route.values.at("legs").get<Array>().values.at(0).get<Object>();
    auto &nodes = leg.values.at("annotation").get<Object>().values.at("nodes").get<Array>();
    std::vector<unsigned> out;
    out.reserve(nodes.values.size());
    for (const auto &v : nodes.values)
        out.push_back(static_cast<unsigned>(v.get<osrm::json::Number>().value) - 1);
    return out;
}

static std::uint64_t price(const Input &g, const std::vector<unsigned> &p, unsigned s, unsigned t,
                           const std::unordered_map<std::uint64_t, unsigned> &arcs, int slot) {
    if (p.empty() || p.front() != s || p.back() != t)
        throw std::runtime_error("OSRM snapped away from a requested node");
    std::uint64_t cost = 0;
    for (auto v = p.begin(); v + 1 != p.end(); ++v) {
        auto a = arcs.find(key(*v, *(v + 1)));
        if (a == arcs.end()) throw std::runtime_error("OSRM path uses an absent directed arc");
        cost += g.metric[slot][a->second];
    }
    return cost;
}

static void query(const Input &g, const std::string &graph, const std::string &algorithm,
                  int pass, int slot, const std::string &indices) {
    osrm::EngineConfig config;
    config.storage_config = {graph}; config.use_shared_memory = false;
    config.algorithm = algorithm == "ch" ? osrm::EngineConfig::Algorithm::CH : osrm::EngineConfig::Algorithm::MLD;
    const auto load = Clock::now();
    const osrm::OSRM engine{config};
    const double load_s = std::chrono::duration<double>(Clock::now() - load).count();
    std::unordered_map<std::uint64_t, unsigned> arcs;
    for (unsigned a = 0; a < g.m; ++a) arcs.emplace(key(g.tail[a], g.head[a]), a);
    std::vector<std::uint64_t> costs(g.q, UINT64_MAX);
    std::vector<unsigned> order;
    std::vector<bool> selected(g.q, false);
    std::istringstream parts(indices);
    std::string part;
    while (std::getline(parts, part, ',')) {
        unsigned i = std::stoul(part);
        if (i >= g.q || selected[i] || (i + pass) % 8 != static_cast<unsigned>(slot))
            throw std::runtime_error("invalid timed query schedule");
        selected[i] = true;
        order.push_back(i);
    }
    const auto population = order.size();
    for (unsigned i = 0; i < g.q; ++i) {
        if ((i + pass) % 8 == static_cast<unsigned>(slot) && !selected[i])
            throw std::runtime_error("incomplete timed query schedule");
        if (!selected[i]) order.push_back(i);
    }
    std::size_t timed = 0, reachable = 0, optimal = 0, identity = 0;
    double query_s = 0;
    for (std::size_t at = 0; at < order.size(); ++at) {
        const auto i = order[at];
        unsigned s = g.pairs[2 * i], t = g.pairs[2 * i + 1];
        osrm::RouteParameters params;
        params.coordinates = {g.xy(s), g.xy(t)};
        params.radiuses = {0.1, 0.1};
        params.overview = osrm::RouteParameters::OverviewType::False;
        params.annotations = true;
        params.annotations_type = osrm::RouteParameters::AnnotationsType::Nodes;
        params.continue_straight = false;
        osrm::engine::api::ResultT result = Object();
        auto start = Clock::now();
        auto status = engine.Route(params, result);
        std::vector<unsigned> p;
        if (status == osrm::Status::Ok) p = path(result);
        const double seconds = std::chrono::duration<double>(Clock::now() - start).count();
        if (at < population) { query_s += seconds; ++timed; }
        const auto reference = g.ref[slot][i];
        if (s == t) {
            ++identity;
            if (status != osrm::Status::Ok) throw std::runtime_error("OSRM rejected an identical-endpoint request");
            costs[i] = 0;
        } else if (status == osrm::Status::Ok) {
            costs[i] = price(g, p, s, t, arcs, slot);
        } else {
            const auto code = result.get<Object>().values.at("code").get<osrm::json::String>().value;
            if (code != "NoRoute") throw std::runtime_error("OSRM query failed: " + code);
        }
        if ((reference == UINT32_MAX) != (costs[i] == UINT64_MAX))
            throw std::runtime_error("OSRM reachability differs from the original graph");
        if (reference != UINT32_MAX) {
            ++reachable;
            if (costs[i] < reference) throw std::runtime_error("OSRM re-priced path beats certified distance");
            optimal += costs[i] == reference;
        }
    }
    struct rusage usage{};
    getrusage(RUSAGE_SELF, &usage);
    std::cout << std::setprecision(12)
              << "{\"kind\":\"industry_query\",\"method\":\"osrm-" << algorithm << "\",\"commit\":\"" << OSRM_COMMIT
              << "\",\"pass\":" << pass << ",\"slot\":" << slot << ",\"queries\":" << g.q
              << ",\"timed_queries\":" << timed << ",\"query_s\":" << query_s
              << ",\"load_s\":" << load_s << ",\"memory_bytes\":" << usage.ru_maxrss * 1024ull
              << ",\"reachable\":" << reachable << ",\"identity\":" << identity
              << ",\"optimal_count\":" << optimal << ",\"reference_ms\":[";
    for (std::size_t i = 0; i < g.q; ++i) {
        if (i) std::cout << ',';
        if (g.ref[slot][i] == UINT32_MAX) std::cout << "null"; else std::cout << g.ref[slot][i];
    }
    std::cout << "],\"price_ms\":[";
    for (std::size_t i = 0; i < g.q; ++i) {
        if (i) std::cout << ',';
        if (costs[i] == UINT64_MAX) std::cout << "null"; else std::cout << costs[i];
    }
    std::cout << "],\"answer_status\":\"re-priced, not certified\"}\n";
}

int main(int argc, char **argv) {
    try {
        if (argc < 4) throw std::runtime_error("usage: osrm-bench segments INPUT DIR | audit INPUT GRAPH SLOT | query INPUT GRAPH ch|mld PASS SLOT QUERY_INDICES");
        const Input g(argv[2]);
        const std::string mode = argv[1];
        if (mode == "segments" && argc == 4) segments(g, argv[3]);
        else if (mode == "audit" && argc == 5) audit(g, argv[3], std::stoi(argv[4]));
        else if (mode == "query" && argc == 8) query(g, argv[3], argv[4], std::stoi(argv[5]), std::stoi(argv[6]), argv[7]);
        else throw std::runtime_error("invalid osrm-bench command");
        return 0;
    } catch (const std::exception &e) {
        std::cerr << "osrm-bench: " << e.what() << '\n';
        return 1;
    }
}
