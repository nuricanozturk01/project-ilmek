package dev.ilmek.sdk.web;

import java.io.IOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashSet;
import java.util.Set;

final class ListeningPorts {

    private static final String LISTEN = "0A";

    private ListeningPorts() {
    }

    static boolean available() {
        return Files.isReadable(Path.of("/proc/net/tcp")) && Files.isReadable(Path.of("/proc/self/fd"));
    }

    static Set<Integer> ofThisProcess() throws IOException {
        Set<String> inodes = socketInodes();
        Set<Integer> ports = new HashSet<>();
        for (String table : new String[] {"/proc/net/tcp", "/proc/net/tcp6"}) {
            Path path = Path.of(table);
            if (!Files.isReadable(path)) {
                continue;
            }
            for (String line : Files.readAllLines(path).stream().skip(1).toList()) {
                String[] columns = line.trim().split("\\s+");
                if (columns.length > 9 && LISTEN.equals(columns[3]) && inodes.contains(columns[9])) {
                    ports.add(Integer.parseInt(columns[1].substring(columns[1].indexOf(':') + 1), 16));
                }
            }
        }
        return ports;
    }

    private static Set<String> socketInodes() throws IOException {
        Set<String> inodes = new HashSet<>();
        try (DirectoryStream<Path> fds = Files.newDirectoryStream(Path.of("/proc/self/fd"))) {
            for (Path fd : fds) {
                try {
                    String target = Files.readSymbolicLink(fd).toString();
                    if (target.startsWith("socket:[")) {
                        inodes.add(target.substring("socket:[".length(), target.length() - 1));
                    }
                } catch (IOException e) {
                    continue;
                }
            }
        }
        return inodes;
    }
}
