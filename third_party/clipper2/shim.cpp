// Minimal C ABI over Clipper2 for build-a-board: the union that KiCad's
// SHAPE_POLY_SET::booleanOp( Union, empty ) performs, returning the PolyTree
// flattened in pre-order so Rust can rebuild KiCad's importTree() result.
#include <cstddef>
#include <cstdint>
#include <memory>
#include <vector>

#include "clipper2/clipper.h"

namespace
{
struct Node
{
    std::ptrdiff_t       parent;
    bool                 hole;
    std::vector<int64_t> xy;
};

struct Result
{
    std::vector<Node> nodes;
};

void flatten( const Clipper2Lib::PolyPath64& path, std::ptrdiff_t parent, Result& out )
{
    for( const std::unique_ptr<Clipper2Lib::PolyPath64>& child : path )
    {
        Node node{ parent, child->IsHole(), {} };
        node.xy.reserve( child->Polygon().size() * 2 );

        for( const Clipper2Lib::Point64& pt : child->Polygon() )
        {
            node.xy.push_back( pt.x );
            node.xy.push_back( pt.y );
        }

        out.nodes.push_back( std::move( node ) );
        flatten( *child, static_cast<std::ptrdiff_t>( out.nodes.size() ) - 1, out );
    }
}
} // namespace

extern "C"
{
// `xy` holds every path's points as x,y pairs back to back; `lens[i]` is the
// point count of path i. Z values are numbered per point like KiCad's
// convertToClipper2 does (they carry no geometry here: there are no arcs).
void* bb_clipper_union( const int64_t* xy, const size_t* lens, size_t count )
{
    Clipper2Lib::Paths64 paths;
    paths.reserve( count );
    size_t  offset = 0;
    int64_t z = 0;

    for( size_t i = 0; i < count; i++ )
    {
        Clipper2Lib::Path64 path;
        path.reserve( lens[i] );

        for( size_t j = 0; j < lens[i]; j++, offset++ )
            path.emplace_back( xy[2 * offset], xy[2 * offset + 1], z++ );

        paths.push_back( std::move( path ) );
    }

    Clipper2Lib::Clipper64 clipper;
    clipper.AddSubject( paths );
    clipper.AddClip( Clipper2Lib::Paths64() );

    Clipper2Lib::PolyTree64 solution;
    clipper.Execute( Clipper2Lib::ClipType::Union, Clipper2Lib::FillRule::NonZero, solution );

    Result* result = new Result();
    flatten( solution, -1, *result );
    solution.Clear();
    return result;
}

size_t bb_clipper_node_count( const void* result )
{
    return static_cast<const Result*>( result )->nodes.size();
}

// Returns the node's point count and points `*xy` at its x,y pairs.
size_t bb_clipper_node( const void* result, size_t index, std::ptrdiff_t* parent, int* hole,
                        const int64_t** xy )
{
    const Node& node = static_cast<const Result*>( result )->nodes[index];
    *parent = node.parent;
    *hole = node.hole ? 1 : 0;
    *xy = node.xy.data();
    return node.xy.size() / 2;
}

void bb_clipper_free( void* result )
{
    delete static_cast<Result*>( result );
}
}
