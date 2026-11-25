package benchmark;

import com.graphhopper.GraphHopper;
import com.graphhopper.GraphHopperConfig;
import com.graphhopper.config.CHProfile;
import com.graphhopper.config.Profile;
import com.graphhopper.json.Statement;
import com.graphhopper.routing.Path;
import com.graphhopper.routing.ch.CHRoutingAlgorithmFactory;
import com.graphhopper.routing.ev.*;
import com.graphhopper.routing.util.EncodingManager;
import com.graphhopper.routing.util.OSMParsers;
import com.graphhopper.util.CustomModel;
import com.graphhopper.util.EdgeIteratorState;
import com.graphhopper.util.PMap;

import java.io.BufferedInputStream;
import java.io.DataInputStream;
import java.io.FileInputStream;
import java.io.IOException;
import java.util.Arrays;
import java.util.List;
import java.util.Map;

/** Native CH queries on the directed arc PBF, with costs re-priced on the original milliseconds.
 * INPUT: scale's routingkit.bin, its exported PBF, a fresh cache directory, pass and slot.
 * OUTPUT: one JSON measurement, retaining every reachable request's integer excess milliseconds.
 * GraphHopper 11.0 uses the upstream car speed encoding: seven bits, 2 km/h per step.
 */
public final class Road {
    static final String COMMIT = "69e50f6e2cfaf0a8e69752df9953ee5f1ac276a4";
    static final long INF = 0xffffffffL;

    static final class Input {
        int n, m, q;
        int[] tail, head, pairs;
        int[][] weights, reference;

        static int[] ints(DataInputStream in, int n) throws IOException {
            int[] a = new int[n];
            for (int i = 0; i < n; i++) a[i] = Integer.reverseBytes(in.readInt());
            return a;
        }

        Input(String path) throws IOException {
            try (DataInputStream in = new DataInputStream(new BufferedInputStream(new FileInputStream(path)))) {
                n = Math.toIntExact(Long.reverseBytes(in.readLong()));
                m = Math.toIntExact(Long.reverseBytes(in.readLong()));
                tail = ints(in, m);
                head = ints(in, m);
                in.skipNBytes(8L * n); // Original f32 coordinates remain in PBF.
                weights = new int[8][];
                for (int j = 0; j < 8; j++) weights[j] = ints(in, m);
                q = Math.toIntExact(Long.reverseBytes(in.readLong()));
                pairs = ints(in, 2 * q);
                reference = new int[8][];
                for (int j = 0; j < 8; j++) reference[j] = ints(in, q);
            }
        }
    }

    static final class Hopper extends GraphHopper {
        final Input input;
        final int slot;
        final int[] arcEdge;
        int[] edgeArc;
        int clamped;

        Hopper(Input input, int slot) {
            this.input = input;
            this.slot = slot;
            arcEdge = new int[input.m];
            Arrays.fill(arcEdge, -1);
            edgeArc = new int[Math.max(1, input.m)];
            Arrays.fill(edgeArc, -1);
        }

        @Override
        protected EncodingManager buildEncodingManager(Map<String, PMap> values,
                Map<String, ImportUnit> units, Map<String, List<String>> restrictions) {
            EncodingManager.Builder builder = new EncodingManager.Builder()
                    .add(VehicleAccess.create("car"))
                    .add(VehicleSpeed.create("car", 7, 2, true));
            createSubnetworkEncodedValues().forEach(builder::add);
            return builder.build();
        }

        @Override
        protected OSMParsers buildOSMParsers(Map<String, PMap> values, Map<String, ImportUnit> units,
                Map<String, List<String>> restrictions, List<String> ignored) {
            BooleanEncodedValue access = getEncodingManager().getBooleanEncodedValue("car_access");
            DecimalEncodedValue speed = getEncodingManager().getDecimalEncodedValue("car_average_speed");
            return new OSMParsers().addWayTagParser((edge, flags, way, relation) -> {
                int arc = Math.toIntExact(way.getId() - 1);
                if (arc < 0 || arc >= input.m || arcEdge[arc] != -1 || edge >= edgeArc.length)
                    throw new IllegalStateException("OSM import split or duplicated original arc " + arc);
                long ms = Long.parseLong(way.getTag("arc_ms_" + slot));
                if (ms != Integer.toUnsignedLong(input.weights[slot][arc]) || ms == 0)
                    throw new IllegalStateException("PBF metric differs from input arc " + arc);
                double distance = way.getTag("edge_distance", Double.NaN);
                if (!Double.isFinite(distance) || distance <= 0)
                    throw new IllegalStateException("Missing edge distance for arc " + arc);
                double kmh = 3600 * distance / ms;
                double bounded = Math.max(speed.getSmallestNonZeroValue(), Math.min(speed.getMaxStorableDecimal(), kmh));
                if (bounded != kmh) clamped++;
                access.setBool(false, edge, flags, true);
                access.setBool(true, edge, flags, false);
                speed.setDecimal(false, edge, flags, bounded);
                speed.setDecimal(true, edge, flags, bounded);
                arcEdge[arc] = edge;
                edgeArc[edge] = arc;
            });
        }

        int[] originalNodes() {
            int[] nodes = new int[input.n];
            Arrays.fill(nodes, -1);
            if (getBaseGraph().getEdges() != input.m)
                throw new IllegalStateException("OSM import changed arc population");
            for (int arc = 0; arc < input.m; arc++) {
                if (arcEdge[arc] < 0) throw new IllegalStateException("OSM import omitted arc " + arc);
                EdgeIteratorState edge = getBaseGraph().getEdgeIteratorState(arcEdge[arc], Integer.MIN_VALUE);
                assign(nodes, input.tail[arc], edge.getBaseNode());
                assign(nodes, input.head[arc], edge.getAdjNode());
            }
            return nodes;
        }

        static void assign(int[] nodes, int original, int imported) {
            if (nodes[original] != -1 && nodes[original] != imported)
                throw new IllegalStateException("Original node was split: " + original);
            nodes[original] = imported;
        }

        String importedUnits() {
            if (input.m > 64) return "[]";
            StringBuilder rows = new StringBuilder("[");
            DecimalEncodedValue speed = getEncodingManager().getDecimalEncodedValue("car_average_speed");
            for (int a = 0; a < input.m; a++) {
                EdgeIteratorState edge = getBaseGraph().getEdgeIteratorState(arcEdge[a], Integer.MIN_VALUE);
                if (a > 0) rows.append(',');
                rows.append('[').append(a).append(',').append(input.tail[a]).append(',')
                        .append(input.head[a]).append(',').append(Integer.toUnsignedLong(input.weights[slot][a]))
                        .append(',').append(edge.getDistance()).append(',').append(edge.get(speed))
                        .append(',').append(Math.round(edge.getDistance() * 3600 / edge.get(speed))).append(']');
            }
            return rows.append(']').toString();
        }
    }

    static double seconds(long start) { return (System.nanoTime() - start) * 1e-9; }

    static final class Shuffle {
        long z;
        Shuffle(int pass) { z = 20261125L + pass; }
        void apply(int[] a) {
            for (int i = a.length - 1; i > 0; i--) {
                z += 0x9e3779b97f4a7c15L;
                long x = z;
                x = (x ^ (x >>> 30)) * 0xbf58476d1ce4e5b9L;
                x = (x ^ (x >>> 27)) * 0x94d049bb133111ebL;
                int j = (int) Long.remainderUnsigned(x ^ (x >>> 31), i + 1L);
                int t = a[i]; a[i] = a[j]; a[j] = t;
            }
        }
    }

    static int[] schedule(int q, int pass, int slot) {
        Shuffle rng = new Shuffle(pass);
        int[] order = {0, 1, 2, 3, 4, 5, 6, 7}, selected = {};
        rng.apply(order);
        for (int s : order) {
            int[] mine = new int[q];
            int n = 0;
            for (int i = 0; i < q; i++) if ((i + pass) % 8 == s) mine[n++] = i;
            mine = Arrays.copyOf(mine, n);
            rng.apply(mine);
            if (s == slot) selected = mine;
        }
        return selected;
    }

    static String json(long[] values) {
        StringBuilder out = new StringBuilder("[");
        for (int i = 0; i < values.length; i++) {
            if (i > 0) out.append(',');
            if (values[i] < 0) out.append("null"); else out.append(values[i]);
        }
        return out.append(']').toString();
    }

    record Answer(long price, double seconds, long nodes) {}

    static Answer answer(Input input, Hopper hopper, int[] nodes, CHRoutingAlgorithmFactory factory, int slot, int i) {
        int s = input.pairs[2 * i], t = input.pairs[2 * i + 1];
        long start = System.nanoTime();
        List<EdgeIteratorState> edges = List.of();
        boolean found = s == t;
        if (nodes[s] >= 0 && nodes[t] >= 0) {
            Path path = factory.createAlgo(new PMap()).calcPath(nodes[s], nodes[t]);
            found = path.isFound();
            edges = path.calcEdges();
        }
        double elapsed = seconds(start);
        long reference = Integer.toUnsignedLong(input.reference[slot][i]);
        if (found != (reference != INF)) throw new IllegalStateException("Reachability changed for query " + i);
        if (!found) return new Answer(-1, elapsed, 0);
        long cost = 0;
        int at = s;
        for (EdgeIteratorState edge : edges) {
            int arc = hopper.edgeArc[edge.getEdge()];
            if (arc < 0 || input.tail[arc] != at)
                throw new IllegalStateException("Path traverses an illegal original arc");
            at = input.head[arc];
            cost += Integer.toUnsignedLong(input.weights[slot][arc]);
        }
        if (at != t || cost < reference) throw new IllegalStateException("Invalid re-priced path " + i);
        return new Answer(cost, elapsed, edges.size() + 1L);
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 5) throw new IllegalArgumentException("Road INPUT PBF FRESH_CACHE PASS SLOT");
        Input input = new Input(args[0]);
        int pass = Integer.parseInt(args[3]), slot = Integer.parseInt(args[4]);
        Hopper hopper = new Hopper(input, slot);
        CustomModel model = new CustomModel().setDistanceInfluence(0d)
                .addToSpeed(Statement.If("true", Statement.Op.LIMIT, "car_average_speed"))
                .addToPriority(Statement.If("!car_access", Statement.Op.MULTIPLY, "0"));
        GraphHopperConfig config = new GraphHopperConfig()
                .putObject("datareader.file", args[1])
                .putObject("graph.location", args[2])
                .putObject("graph.dataaccess.default_type", "RAM_STORE")
                .putObject("graph.encoded_values", "car_access,car_average_speed")
                .putObject("import.osm.ignored_highways", "")
                .putObject("prepare.min_network_size", 0)
                .putObject("datareader.worker_threads", 1)
                .putObject("prepare.subnetworks.threads", 1)
                .putObject("prepare.ch.threads", 1)
                .setProfiles(List.of(new Profile("car").setCustomModel(model)))
                .setCHProfiles(List.of(new CHProfile("car")));
        hopper.init(config);
        long start = System.nanoTime();
        hopper.importOrLoad();
        double update = seconds(start);
        int[] nodes = hopper.originalNodes();
        CHRoutingAlgorithmFactory factory = new CHRoutingAlgorithmFactory(hopper.getCHGraphs().get("car"));
        int[] selected = schedule(input.q, pass, slot);
        boolean[] timed = new boolean[input.q];
        for (int i : selected) timed[i] = true;
        int[] order = new int[input.q];
        System.arraycopy(selected, 0, order, 0, selected.length);
        int next = selected.length;
        for (int i = 0; i < input.q; i++) if (!timed[i]) order[next++] = i;
        long[] excess = new long[selected.length];
        long[] references = new long[input.q], prices = new long[input.q];
        Arrays.fill(prices, -1);
        int reachable = 0, checkedReachable = 0, checkedOptimal = 0, identity = 0;
        long pathNodes = 0;
        double query = 0;
        for (int i : order) {
            long reference = Integer.toUnsignedLong(input.reference[slot][i]);
            references[i] = reference == INF ? -1 : reference;
            Answer answer = answer(input, hopper, nodes, factory, slot, i);
            pathNodes += answer.nodes();
            if (timed[i]) query += answer.seconds();
            if (answer.price() < 0) continue;
            if (input.pairs[2 * i] == input.pairs[2 * i + 1]) identity++;
            prices[i] = answer.price();
            checkedReachable++;
            if (answer.price() == reference) checkedOptimal++;
            if (timed[i]) excess[reachable++] = answer.price() - reference;
        }
        excess = Arrays.copyOf(excess, reachable);
        System.out.printf(java.util.Locale.ROOT,
                "{\"kind\":\"industry_query\",\"method\":\"graphhopper-ch\",\"release\":\"11.0\",\"commit\":\"%s\","
                + "\"pass\":%d,\"slot\":%d,\"queries\":%d,\"timed_queries\":%d,\"reachable\":%d,\"identity\":%d,"
                + "\"query_s\":%.12g,\"update_s\":%.12g,\"prepare_s\":0,\"threads\":1,"
                + "\"optimal_count\":%d,\"excess_ms\":%s,\"clamped_arcs\":%d,\"speed_step_kmh\":2,"
                + "\"path_nodes\":%d,\"arcs\":%d,\"imported_arcs\":%d,\"checked\":%d,"
                + "\"imported_units\":%s,\"reference_ms\":%s,\"price_ms\":%s,"
                + "\"answer_status\":\"re-priced, not certified\",\"wrong\":0}%n",
                COMMIT, pass, slot, input.q, selected.length, checkedReachable, identity, query, update,
                checkedOptimal, Arrays.toString(excess), hopper.clamped, pathNodes, input.m, hopper.getBaseGraph().getEdges(), input.q,
                hopper.importedUnits(), json(references), json(prices));
        hopper.close();
    }
}
