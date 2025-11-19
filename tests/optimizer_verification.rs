use rustgresql::optimizer::OptimizedQueryPlanner;
use rustgresql::sql::ast::*;
use rustgresql::executor::planner::PlanNode;
use rustgresql::types::{Value, ValueKind};

#[test]
fn test_predicate_pushdown() {
    let planner = OptimizedQueryPlanner::new();
    
    // Construct a SELECT statement: SELECT * FROM t1 JOIN t2 ON t1.id = t2.id WHERE t1.id = 10
    // PredicatePushdown should push `t1.id = 10` down to t1 scan.
    
    let select = SelectStatement::Simple {
        with_clause: None,
        distinct: false,
        columns: vec![ColumnSpec {
            expr: Expression::Star,
            alias: None,
        }],
        from: vec![TableRef { name: "t1".to_string(), alias: None }],
        joins: vec![Join {
            table: TableRef { name: "t2".to_string(), alias: None },
            join_type: JoinType::Inner,
            condition: Some(Expression::BinaryOp {
                left: Box::new(Expression::Column { name: "id".to_string(), table: Some("t1".to_string()) }),
                op: BinaryOperator::Equals,
                right: Box::new(Expression::Column { name: "id".to_string(), table: Some("t2".to_string()) }),
            }),
        }],
        where_clause: Some(Expression::BinaryOp {
             left: Box::new(Expression::Column { name: "id".to_string(), table: Some("t1".to_string()) }),
             op: BinaryOperator::Equals,
             right: Box::new(Expression::Value(Value { kind: ValueKind::Integer(10) })),
        }),
        group_by: vec![],
        having: None,
        order_by: vec![],
        limit: None,
        offset: None,
        named_windows: vec![],
    };

    let indexes = vec![];
    let plan = planner.plan_select(&select, &indexes).unwrap();
    
    // Traverse plan to verify structure
    // Root should be Project (added by planner) -> Join
    
    let join_node = if let PlanNode::Project { input, .. } = plan.root {
        *input
    } else {
        plan.root
    };

    if let PlanNode::Join { left, .. } = join_node {
        // Check if Filter is on the left side (pushed down to t1)
        // The planner might put Scan directly if it optimized it into IndexScan, but we provided no indexes.
        // So it should be Filter -> Scan
        
        match *left {
             PlanNode::Filter { input, .. } => {
                 // And input is Scan t1
                 if let PlanNode::Scan { table_name, .. } = *input {
                     assert_eq!(table_name, "t1");
                 } else {
                     panic!("Expected Scan under Filter on left side, got {:?}", input);
                 }
             }
             _ => panic!("Expected Filter on left side due to pushdown, got {:?}", left),
        }
#[test]
fn test_constant_folding() {
    let planner = OptimizedQueryPlanner::new();
    
    // Construct a SELECT statement: SELECT 1 + 2
    // ConstantFolding should simplify 1 + 2 to 3
    
    let select = SelectStatement::Simple {
        with_clause: None,
        distinct: false,
        columns: vec![ColumnSpec {
            expr: Expression::BinaryOp {
                left: Box::new(Expression::Value(Value { kind: ValueKind::Integer(1) })),
                op: BinaryOperator::Add,
                right: Box::new(Expression::Value(Value { kind: ValueKind::Integer(2) })),
            },
            alias: Some("result".to_string()),
        }],
        from: vec![], // No FROM clause
        joins: vec![],
        where_clause: None,
        group_by: vec![],
        having: None,
        order_by: vec![],
        limit: None,
        offset: None,
        named_windows: vec![],
    };

    let indexes = vec![];
    let plan = planner.plan_select(&select, &indexes).unwrap();
    
    // Verify plan structure
    // Root should be Project (with simplified expression) -> Scan (dummy/empty)
    
    if let PlanNode::Project { columns, .. } = plan.root {
        assert_eq!(columns.len(), 1);
        let (name, expr) = &columns[0];
        assert_eq!(name, "result");
        
        // Expression should be simplified to Value(3)
        if let Expression::Value(val) = expr {
             if let ValueKind::Integer(i) = val.kind {
                 assert_eq!(i, 3);
             } else {
                 panic!("Expected Integer(3), got {:?}", val);
             }
        } else {
            panic!("Expected simplified Value expression, got {:?}", expr);
        }
    } else {
        panic!("Expected Project at root, got {:?}", plan.root);
    }
}
