// RoutingKit's CCH or CH on the graph, metrics and workload of one `scale partial` or `scale export`
// run: the external engines compared with this repository's.
//
// Usage: routingkit INPUT cch|ch, where INPUT is the file `scale` writes (layout in
// src/bin/scale.rs). RoutingKit (https://github.com/RoutingKit/RoutingKit, BSD-2-Clause) is built in
// CI at the commit pinned in .github/workflows/scale.yml.
//
// Every answer returns its node path, and each distance is checked against the certified distance
// in INPUT. ch: for three passes over the eight weekday metrics, build a contraction hierarchy
// (RoutingKit's own witness-search contraction, rebuilt per metric), then answer every query.
// cch: its own inertial-flow nested dissection orders the nodes once. For three passes over the
// eight weekday metrics: customize on one thread and on four (wall seconds), then answer every
// query. For three passes over the synthetic updates: customize the source metric, then time a partial
// customization over the changed arcs and a full one of the same weights; the first ten answers
// of the partial metric must equal those of the full one. One JSON row per measurement on stdout.

#include <routingkit/contraction_hierarchy.h>
#include <routingkit/customizable_contraction_hierarchy.h>
#include <routingkit/nested_dissection.h>

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iterator>
#include <vector>
#include <algorithm>
#include <utility>

using namespace RoutingKit;

static double now() {
    return std::chrono::duration<double>(std::chrono::steady_clock::now().time_since_epoch()).count();
}

// The same SplitMix64 schedule as scale.rs: every request is timed once per pass.
template<class T> static void shuffle(std::vector<T>& x, uint64_t& z) {
    for (size_t i = x.size(); i > 1; --i) {
        uint64_t a = (z += 0x9e3779b97f4a7c15ULL);
        a = (a ^ (a >> 30)) * 0xbf58476d1ce4e5b9ULL;
        a = (a ^ (a >> 27)) * 0x94d049bb133111ebULL;
        std::swap(x[i - 1], x[(a ^ (a >> 31)) % i]);
    }
}

static auto schedule(uint64_t q, int pass) {
    uint64_t z = 20261125 + pass;
    std::vector<int> slots{0,1,2,3,4,5,6,7};
    shuffle(slots, z);
    std::vector<std::pair<int, std::vector<uint64_t>>> out;
    for (int slot : slots) {
        std::vector<uint64_t> mine;
        for (uint64_t i = 0; i < q; ++i)
            if ((i + pass) % 8 == static_cast<unsigned>(slot)) mine.push_back(i);
        shuffle(mine, z);
        out.emplace_back(slot, std::move(mine));
    }
    return out;
}

struct Reader {
    std::vector<char> b;
    size_t at = 0;
    template <class T> T one() {
        T x;
        std::memcpy(&x, b.data() + at, sizeof(T));
        at += sizeof(T);
        return x;
    }
    template <class T> std::vector<T> many(uint64_t n) {
        std::vector<T> x(n);
        std::memcpy(x.data(), b.data() + at, n * sizeof(T));
        at += n * sizeof(T);
        return x;
    }
};

int main(int argc, char** argv) {
    if (argc != 3 || (std::strcmp(argv[2], "cch") && std::strcmp(argv[2], "ch"))) {
        std::fprintf(stderr, "usage: routingkit INPUT cch|ch\n");
        return 2;
    }
    std::ifstream in(argv[1], std::ios::binary);
    Reader r{std::vector<char>(std::istreambuf_iterator<char>(in), {})};
    const uint64_t n = r.one<uint64_t>(), m = r.one<uint64_t>();
    const auto tail = r.many<unsigned>(m), head = r.many<unsigned>(m);
    const auto lat = r.many<float>(n), lon = r.many<float>(n);
    std::vector<std::vector<unsigned>> metric(8);
    for (auto& w : metric) w = r.many<unsigned>(m);
    const uint64_t q = r.one<uint64_t>();
    const auto pairs = r.many<unsigned>(2 * q);
    std::vector<std::vector<unsigned>> ref(8);
    for (auto& d : ref) d = r.many<unsigned>(q);
    struct Update { unsigned from, to; double f; std::vector<unsigned> arcs; };
    std::vector<Update> ups(r.one<uint64_t>());
    for (auto& u : ups) {
        u.from = r.one<unsigned>();
        u.to = r.one<unsigned>();
        u.f = r.one<double>();
        u.arcs = r.many<unsigned>(r.one<uint64_t>());
    }

    if (!std::strcmp(argv[2], "ch")) {
        uint64_t nodes = 0;
        for (int pass = 0; pass < 3; ++pass) {
            for (const auto& [j, mine] : schedule(q, pass)) {
                double t = now();
                const auto ch = ContractionHierarchy::build(n, tail, head, metric[j]);
                const double build = now() - t;
                ContractionHierarchyQuery query(ch);
                t = now();
                uint64_t wrong = 0;
                auto distance = [&](uint64_t i) {
                    query.reset().add_source(pairs[2 * i]).add_target(pairs[2 * i + 1]).run();
                    unsigned d = query.get_distance();
                    if (d != inf_weight) nodes += query.get_node_path().size();
                    return d == inf_weight ? UINT32_MAX : d;
                };
                for (auto i : mine) wrong += distance(i) != ref[j][i];
                const double query_s = now() - t;
                for (uint64_t i = 0; i < q; ++i) wrong += distance(i) != ref[j][i];
                if (wrong) {
                    std::fprintf(stderr, "RoutingKit CH: %llu wrong on slot %d\n", (unsigned long long)wrong, j);
                    return 1;
                }
                std::printf("{\"kind\":\"routingkit_ch_slot\",\"commit\":\"%s\",\"pass\":%d,\"slot\":%d,"
                            "\"build_s\":%.9g,\"queries\":%llu,\"query_s\":%.9g,\"checked\":%llu,\"wrong\":0,"
                            "\"timing_contract\":\"paths-reverse-update-v1\"}\n",
                            ROUTINGKIT_COMMIT, pass, j, build, (unsigned long long)mine.size(), query_s, (unsigned long long)q);
                std::fflush(stdout);
            }
        }
        std::fprintf(stderr, "RoutingKit CH: %llu path nodes\n", (unsigned long long)nodes);
        return 0;
    }

    double t = now();
    const auto order = compute_nested_node_dissection_order_using_inertial_flow(
        n, tail, head, std::vector<float>(lat), std::vector<float>(lon));
    const double order_s = now() - t;
    t = now();
    CustomizableContractionHierarchy cch(order, tail, head);
    const double build_s = now() - t;
    std::printf("{\"kind\":\"routingkit_prepare\",\"commit\":\"%s\",\"order_s\":%.9g,\"build_s\":%.9g,"
                "\"nodes\":%llu,\"arcs\":%llu}\n", ROUTINGKIT_COMMIT, order_s, build_s,
                (unsigned long long)n, (unsigned long long)m);
    CustomizableContractionHierarchyParallelization parallel(cch);
    CustomizableContractionHierarchyPartialCustomization partial(cch);
    std::vector<unsigned> w = metric[0];
    CustomizableContractionHierarchyMetric cm(cch, w);
    CustomizableContractionHierarchyQuery query(cm);
    // Each answer returns its node path as ours do; `nodes` keeps the work observable.
    uint64_t nodes = 0;
    auto distance = [&](uint64_t i) {
        query.reset().add_source(pairs[2 * i]).add_target(pairs[2 * i + 1]).run();
        const unsigned d = query.get_distance();
        if (d != inf_weight) nodes += query.get_node_path().size();
        return d == inf_weight ? UINT32_MAX : d;
    };
    for (int pass = 0; pass < 3; ++pass) {
        for (const auto& [j, mine] : schedule(q, pass)) {
            w = metric[j];
            t = now();
            cm.customize();
            const double one = now() - t;
            t = now();
            parallel.customize(cm, 4);
            const double four = now() - t;
            t = now();
            uint64_t wrong = 0;
            for (auto i : mine) wrong += distance(i) != ref[j][i];
            const double query_s = now() - t;
            for (uint64_t i = 0; i < q; ++i) wrong += distance(i) != ref[j][i];
            if (wrong) {
                std::fprintf(stderr, "RoutingKit: %llu wrong on slot %d\n", (unsigned long long)wrong, j);
                return 1;
            }
            std::printf("{\"kind\":\"routingkit_slot\",\"pass\":%d,\"slot\":%d,\"customize_s\":%.9g,"
                        "\"customize4_wall_s\":%.9g,\"queries\":%llu,\"query_s\":%.9g,\"checked\":%llu,"
                        "\"wrong\":0,\"timing_contract\":\"paths-reverse-update-v1\"}\n", pass, j, one, four, (unsigned long long)mine.size(), query_s,
                        (unsigned long long)q);
        }
        for (const auto& u : ups) {
            w = metric[u.from - 1];
            cm.customize();
            for (unsigned a : u.arcs) w[a] = metric[u.to - 1][a];
            t = now();
            partial.reset();
            for (unsigned a : u.arcs) partial.update_arc(a);
            partial.customize(cm);
            const double partial_s = now() - t;
            const uint64_t k = q < 10 ? q : 10;
            std::vector<unsigned> got(k);
            for (uint64_t i = 0; i < k; ++i) got[i] = distance(i);
            t = now();
            cm.customize();
            const double full_s = now() - t;
            for (uint64_t i = 0; i < k; ++i) {
                if (distance(i) != got[i]) {
                    std::fprintf(stderr, "RoutingKit: partial differs from full\n");
                    return 1;
                }
            }
            std::printf("{\"kind\":\"routingkit_update\",\"pass\":%d,\"from\":%u,\"to\":%u,\"target_f\":%.9g,"
                        "\"changed\":%zu,\"partial_s\":%.9g,\"full_s\":%.9g,\"checked\":%llu,\"wrong\":0}\n",
                        pass, u.from, u.to, u.f, u.arcs.size(), partial_s, full_s, (unsigned long long)k);
        }
    }
    std::fprintf(stderr, "RoutingKit: %llu path nodes\n", (unsigned long long)nodes);
    return 0;
}
